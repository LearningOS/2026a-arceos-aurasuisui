#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]
#![feature(asm_const)]
#![feature(riscv_ext_intrinsics)]

#[cfg(feature = "axstd")]
extern crate axstd as std;
extern crate alloc;
#[macro_use]
extern crate axlog;

mod task;
mod vcpu;
mod regs;
mod csrs;
mod sbi;
mod loader;

use vcpu::VmCpuRegisters;
use riscv::register::{scause, sstatus};
use csrs::defs::hstatus;
use tock_registers::LocalRegisterCopy;
use csrs::{RiscvCsrTrait, CSR};
use vcpu::_run_guest;
use sbi::SbiMessage;
use loader::load_vm_image;
use axhal::mem::{phys_to_virt, PhysAddr, PAGE_SIZE_4K};
use axhal::paging::MappingFlags;
use axmm::AddrSpace;
use crate::regs::GprIndex::{self, A0, A1};

const VM_ENTRY: usize = 0x8020_0000;

/// Hart id that this hypervisor reports to the guest when it reads `mhartid`.
///
/// VS-mode may not access the machine-level `mhartid` CSR, so the read traps to the
/// hypervisor, which answers with the id of the virtual CPU it presents.
const VIRTUAL_HART_ID: usize = 0x1234;

/// Value of the boot-info word that the hypervisor provides to the guest.
///
/// `skernel2` loads its `a0` from guest physical address `BOOT_INFO_GPA` before it
/// asks for a reset, so that word is part of the boot interface of this VM.
const BOOT_INFO_VALUE: usize = 0x6688;

/// Guest physical address of the boot-info word provided by the hypervisor.
const BOOT_INFO_GPA: usize = 0x40;

#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    ax_println!("Hypervisor ...");

    // A new address space for vm.
    let mut uspace = axmm::new_user_aspace().unwrap();

    // Load vm binary file into address space.
    if let Err(e) = load_vm_image("/sbin/skernel2", &mut uspace) {
        panic!("Cannot load app! {:?}", e);
    }

    // Setup context to prepare to enter guest mode.
    let mut ctx = VmCpuRegisters::default();
    prepare_guest_context(&mut ctx);

    // Setup pagetable for 2nd address mapping.
    let ept_root = uspace.page_table_root();
    prepare_vm_pgtable(ept_root);

    // Kick off vm and wait for it to exit.
    while !run_guest(&mut ctx, &mut uspace) {}

    // The guest asked for a system reset: stop the whole machine as well.
    axhal::misc::terminate();
}

fn prepare_vm_pgtable(ept_root: PhysAddr) {
    let hgatp = 8usize << 60 | usize::from(ept_root) >> 12;
    unsafe {
        core::arch::asm!(
            "csrw hgatp, {hgatp}",
            hgatp = in(reg) hgatp,
        );
        core::arch::riscv64::hfence_gvma_all();
    }
}

fn run_guest(ctx: &mut VmCpuRegisters, uspace: &mut AddrSpace) -> bool {
    // Keep HS-mode interrupts disabled while the vCPU runs: `_guest_exit` restores the
    // hypervisor's `sstatus` (which re-enables them) before it restores `stvec`, so a
    // timer interrupt in that window would re-enter the trap handler with the guest's
    // `sscratch` and never return.
    let flags = axhal::arch::local_irq_save_and_disable();
    // `sstatus.SIE` alone is not enough: while `V=1` the HS-level interrupts are not
    // masked by it (the trap we want to avoid is taken in the guest), so the interrupt
    // sources are masked as well. The pending timer interrupt is delivered normally
    // once the guest has been left, right after `sie` and `SIE` are restored.
    let sie = CSR.sie.get_value();
    CSR.sie.write_value(0);
    unsafe {
        _run_guest(ctx);
    }
    CSR.sie.write_value(sie);
    axhal::arch::local_irq_restore(flags);

    vmexit_handler(ctx, uspace)
}

#[allow(unreachable_code)]
fn vmexit_handler(ctx: &mut VmCpuRegisters, uspace: &mut AddrSpace) -> bool {
    use scause::{Exception, Trap};

    // Use the cause captured by `_guest_exit` rather than the live `scause` CSR: the
    // hypervisor handles the exit with interrupts enabled again, so a nested trap (the
    // timer interrupt) may have overwritten it in the meantime.
    let cause = trap_cause(ctx.trap_csrs.scause);
    match cause {
        Trap::Exception(Exception::VirtualSupervisorEnvCall) => {
            let sbi_msg = SbiMessage::from_regs(ctx.guest_regs.gprs.a_regs()).ok();
            ax_println!("VmExit Reason: VSuperEcall: {:?}", sbi_msg);
            if let Some(msg) = sbi_msg {
                match msg {
                    SbiMessage::Reset(_) => {
                        let a0 = ctx.guest_regs.gprs.reg(A0);
                        let a1 = ctx.guest_regs.gprs.reg(A1);
                        ax_println!("a0 = {:#x}, a1 = {:#x}", a0, a1);
                        assert_eq!(a0, 0x6688);
                        assert_eq!(a1, 0x1234);
                        ax_println!("Shutdown vm normally!");
                        return true;
                    },
                    _ => todo!(),
                }
            } else {
                panic!("bad sbi message! ");
            }
        },
        Trap::Exception(Exception::IllegalInstruction) => {
            emulate_illegal_instruction(ctx, uspace);
        },
        Trap::Exception(Exception::LoadGuestPageFault) => {
            handle_guest_page_fault(ctx, uspace);
        },
        _ => {
            panic!(
                "Unhandled trap: {:?}, sepc: {:#x}, stval: {:#x}",
                cause,
                ctx.guest_regs.sepc,
                ctx.trap_csrs.stval
            );
        }
    }
    false
}

/// Emulates an instruction that trapped in VS-mode as an illegal instruction.
///
/// The guest starts with `csrr a1, mhartid`. Machine-level CSRs are not accessible
/// from VS-mode, so the hypervisor decodes the faulting instruction, writes the
/// virtual hart id into its destination register and skips over it.
fn emulate_illegal_instruction(ctx: &mut VmCpuRegisters, uspace: &AddrSpace) {
    let pc = ctx.guest_regs.sepc;
    let inst = read_guest_u32(uspace, pc)
        .unwrap_or_else(|| panic!("Cannot fetch guest instruction at {:#x}", pc));

    let opcode = inst & 0x7f;
    let rd = GprIndex::from_raw((inst >> 7) & 0x1f);
    let funct3 = (inst >> 12) & 0x7;
    let rs1 = (inst >> 15) & 0x1f;
    let csr = inst >> 20;

    ax_println!("VmExit Reason: IllegalInstruction: {:#010x}, sepc: {:#x}", inst, pc);

    match (opcode, funct3, rs1, csr, rd) {
        // csrr <rd>, mhartid  ==  csrrs <rd>, mhartid, x0
        (0x73, 0b010, 0, 0xf14, Some(rd)) => {
            ctx.guest_regs.gprs.set_reg(rd, VIRTUAL_HART_ID);
            ctx.guest_regs.sepc = pc.wrapping_add(4);
        },
        _ => panic!("Unsupported illegal instruction {:#010x} at {:#x}", inst, pc),
    }
}

/// Handles a stage-2 (guest) page fault while the guest was executing.
///
/// The guest runs with `vsatp` = 0, i.e. its addresses are guest physical addresses,
/// so a fault means the EPT (built from the VM address space) has no translation for
/// the GPA that was touched. The hypervisor materialises that page, provides the
/// boot-info word through ordinary guest memory and lets the guest retry the access.
fn handle_guest_page_fault(ctx: &mut VmCpuRegisters, uspace: &mut AddrSpace) {
    let gpa = faulting_gpa(ctx);
    let page = gpa & !(PAGE_SIZE_4K - 1);

    ax_println!(
        "VmExit Reason: LoadGuestPageFault: gpa: {:#x}, sepc: {:#x}",
        gpa,
        ctx.guest_regs.sepc
    );

    // Map the missing page into the EPT so that the faulting access can be retried.
    // The USER flag is required for a stage-2 leaf PTE: the second stage of the walk is
    // a user-level translation, so a supervisor-only entry would be rejected again.
    uspace
        .map_alloc(
            page.into(),
            PAGE_SIZE_4K,
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER,
            true,
        )
        .unwrap_or_else(|e| panic!("Cannot map guest page {:#x}: {:?}", page, e));

    // The boot interface of the guest: a plain load from guest memory provides it.
    if gpa == BOOT_INFO_GPA {
        uspace
            .write(gpa.into(), &(BOOT_INFO_VALUE as u64).to_le_bytes())
            .unwrap_or_else(|e| panic!("Cannot populate guest page {:#x}: {:?}", gpa, e));
    }

    // The stage-2 translation changed: drop cached guest translations before resuming.
    unsafe {
        core::arch::riscv64::hfence_gvma_all();
    }

    // `sepc` is left untouched: the offending instruction is retried and now succeeds.
}

/// Decodes the VM exit cause that `_guest_exit` recorded in `trap_csrs.scause`.
fn trap_cause(cause: usize) -> scause::Trap {
    let interrupt_bit = 1usize << (usize::BITS as usize - 1);
    let code = cause & !interrupt_bit;
    if cause & interrupt_bit != 0 {
        scause::Trap::Interrupt(scause::Interrupt::from(code))
    } else {
        scause::Trap::Exception(scause::Exception::from(code))
    }
}

/// Returns the guest physical address that caused a guest-page fault.
///
/// For a load/store guest-page fault `htval` holds the faulting GPA shifted right by
/// two bits. Since the guest runs without a VS-stage page table, its virtual address
/// (`stval`) is the GPA as well, which is checked here.
fn faulting_gpa(ctx: &VmCpuRegisters) -> usize {
    let gpa = (ctx.trap_csrs.htval & !0x3) << 2;
    assert_eq!(
        gpa, ctx.trap_csrs.stval,
        "guest-page fault: htval and stval disagree"
    );
    gpa
}

/// Reads the 32-bit guest instruction at `gpa` through the stage-2 page table.
fn read_guest_u32(uspace: &AddrSpace, gpa: usize) -> Option<u32> {
    let (paddr, _, _) = uspace.page_table().query(gpa.into()).ok()?;
    let ptr = phys_to_virt(paddr).as_usize() as *const u32;
    Some(unsafe { core::ptr::read_volatile(ptr) })
}

fn prepare_guest_context(ctx: &mut VmCpuRegisters) {
    // Set hstatus
    let mut hstatus = LocalRegisterCopy::<usize, hstatus::Register>::new(
        riscv::register::hstatus::read().bits(),
    );
    // Set Guest bit in order to return to guest mode.
    hstatus.modify(hstatus::spv::Guest);
    // Set SPVP bit in order to accessing VS-mode memory from HS-mode.
    hstatus.modify(hstatus::spvp::Supervisor);
    CSR.hstatus.write_value(hstatus.get());
    ctx.guest_regs.hstatus = hstatus.get();

    // Set sstatus in guest mode.
    let mut sstatus = sstatus::read();
    sstatus.set_spp(sstatus::SPP::Supervisor);
    // Enter the guest with interrupts masked: nothing is delegated to it (`hideleg` = 0),
    // so HS-level interrupts would trap back into the hypervisor. `sret` copies SPIE into
    // SIE, hence both bits are cleared. The interrupt sources themselves are masked in
    // `run_guest` around the `sret`, where `sstatus` alone cannot hold them off.
    let mut guest_sstatus = sstatus.bits();
    guest_sstatus &= !((1 << 1) | (1 << 5)); // SIE | SPIE
    ctx.guest_regs.sstatus = guest_sstatus;
    // Return to entry to start vm.
    ctx.guest_regs.sepc = VM_ENTRY;
}
