use std::io::{self, Read};
use std::fs::File;
use axhal::paging::MappingFlags;
use axhal::mem::{PAGE_SIZE_4K, phys_to_virt};
use axmm::AddrSpace;
use crate::VM_ENTRY;

pub fn load_vm_image(fname: &str, uspace: &mut AddrSpace) -> io::Result<()> {
    // The whole VM image has to fit into the single page that is mapped below.
    let mut buf = [0u8; PAGE_SIZE_4K];
    let size = load_file(fname, &mut buf)?;

    uspace.map_alloc(VM_ENTRY.into(), PAGE_SIZE_4K, MappingFlags::READ|MappingFlags::WRITE|MappingFlags::EXECUTE|MappingFlags::USER, true).unwrap();

    let (paddr, _, _) = uspace
        .page_table()
        .query(VM_ENTRY.into())
        .unwrap_or_else(|_| panic!("Mapping failed for segment: {:#x}", VM_ENTRY));

    ax_println!("paddr: {:#x}", paddr);

    unsafe {
        let dst: *mut u8 = phys_to_virt(paddr).as_mut_ptr();
        // Zero the rest of the page so the guest never sees stale memory contents.
        core::ptr::write_bytes(dst, 0, PAGE_SIZE_4K);
        core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, size);
    }

    Ok(())
}

fn load_file(fname: &str, buf: &mut [u8]) -> io::Result<usize> {
    ax_println!("app: {}", fname);
    let mut file = File::open(fname)?;
    let mut size = 0;
    while size < buf.len() {
        let n = file.read(&mut buf[size..])?;
        if n == 0 {
            return Ok(size);
        }
        size += n;
    }
    // The buffer is full: make sure the image really ends here instead of silently
    // truncating a larger one.
    let mut extra = [0u8; 1];
    if file.read(&mut extra)? != 0 {
        panic!("VM image {} does not fit into one page", fname);
    }
    Ok(size)
}
