#![allow(dead_code)]

use core::ffi::{c_void, c_char, c_int};
use axhal::arch::TrapFrame;
use axhal::trap::{register_trap_handler, SYSCALL};
use axerrno::LinuxError;
use axtask::current;
use axtask::TaskExtRef;
use axhal::mem::{VirtAddr, PAGE_SIZE_4K};
use axhal::paging::MappingFlags;
use memory_addr::VirtAddrRange;
use arceos_posix_api as api;

const SYS_IOCTL: usize = 29;
const SYS_OPENAT: usize = 56;
const SYS_CLOSE: usize = 57;
const SYS_READ: usize = 63;
const SYS_WRITE: usize = 64;
const SYS_WRITEV: usize = 66;
const SYS_EXIT: usize = 93;
const SYS_EXIT_GROUP: usize = 94;
const SYS_SET_TID_ADDRESS: usize = 96;
const SYS_MMAP: usize = 222;

const AT_FDCWD: i32 = -100;

/// Macro to generate syscall body
///
/// It will receive a function which return Result<_, LinuxError> and convert it to
/// the type which is specified by the caller.
#[macro_export]
macro_rules! syscall_body {
    ($fn: ident, $($stmt: tt)*) => {{
        #[allow(clippy::redundant_closure_call)]
        let res = (|| -> axerrno::LinuxResult<_> { $($stmt)* })();
        match res {
            Ok(_) | Err(axerrno::LinuxError::EAGAIN) => debug!(concat!(stringify!($fn), " => {:?}"),  res),
            Err(_) => info!(concat!(stringify!($fn), " => {:?}"), res),
        }
        match res {
            Ok(v) => v as _,
            Err(e) => {
                -e.code() as _
            }
        }
    }};
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// permissions for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapProt: i32 {
        /// Page can be read.
        const PROT_READ = 1 << 0;
        /// Page can be written.
        const PROT_WRITE = 1 << 1;
        /// Page can be executed.
        const PROT_EXEC = 1 << 2;
    }
}

impl From<MmapProt> for MappingFlags {
    fn from(value: MmapProt) -> Self {
        let mut flags = MappingFlags::USER;
        if value.contains(MmapProt::PROT_READ) {
            flags |= MappingFlags::READ;
        }
        if value.contains(MmapProt::PROT_WRITE) {
            flags |= MappingFlags::WRITE;
        }
        if value.contains(MmapProt::PROT_EXEC) {
            flags |= MappingFlags::EXECUTE;
        }
        flags
    }
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// flags for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapFlags: i32 {
        /// Share changes
        const MAP_SHARED = 1 << 0;
        /// Changes private; copy pages on write.
        const MAP_PRIVATE = 1 << 1;
        /// Map address must be exactly as requested, no matter whether it is available.
        const MAP_FIXED = 1 << 4;
        /// Don't use a file.
        const MAP_ANONYMOUS = 1 << 5;
        /// Don't check for reservations.
        const MAP_NORESERVE = 1 << 14;
        /// Allocation is for a stack.
        const MAP_STACK = 0x20000;
    }
}

#[register_trap_handler(SYSCALL)]
fn handle_syscall(tf: &TrapFrame, syscall_num: usize) -> isize {
    ax_println!("handle_syscall [{}] ...", syscall_num);
    let ret = match syscall_num {
         SYS_IOCTL => sys_ioctl(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _) as _,
        SYS_SET_TID_ADDRESS => sys_set_tid_address(tf.arg0() as _),
        SYS_OPENAT => sys_openat(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _, tf.arg3() as _),
        SYS_CLOSE => sys_close(tf.arg0() as _),
        SYS_READ => sys_read(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITE => sys_write(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITEV => sys_writev(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_EXIT_GROUP => {
            ax_println!("[SYS_EXIT_GROUP]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_EXIT => {
            ax_println!("[SYS_EXIT]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_MMAP => sys_mmap(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3() as _,
            tf.arg4() as _,
            tf.arg5() as _,
        ),
        _ => {
            ax_println!("Unimplemented syscall: {}", syscall_num);
            -LinuxError::ENOSYS.code() as _
        }
    };
    ret
}

/// Base of the region reserved for file mappings in the user address space.
///
/// The user app is loaded at VA 0 (and the user stack sits just below the top
/// of the address space), so this area is free for the minimal file mapping.
const MMAP_BASE: usize = 0x2000_0000;
/// Size of the reserved file-mapping region (16 MiB).
const MMAP_MAX_SIZE: usize = 0x1000_0000;

const SEEK_SET: i32 = 0;
const SEEK_CUR: i32 = 1;

/// Map a file into the current task's address space.
///
/// Only the minimal case needed by `payload/mapfile_c` is supported: a
/// read-only, `MAP_PRIVATE`, file-backed mapping at `offset == 0` with
/// `addr == NULL`. The file content is copied into freshly allocated pages and
/// the mapping is returned as `addr`.
fn sys_mmap(
    addr: *mut usize,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: isize,
) -> isize {
    syscall_body!(sys_mmap, {
        let prot = MmapProt::from_bits_truncate(prot);
        let flags = MmapFlags::from_bits_truncate(flags);

        // Only `mmap(NULL, len, PROT_READ, MAP_PRIVATE, fd, 0)` is supported.
        if !flags.contains(MmapFlags::MAP_PRIVATE)
            || flags.intersects(
                MmapFlags::MAP_SHARED | MmapFlags::MAP_ANONYMOUS | MmapFlags::MAP_FIXED,
            )
            || offset != 0
        {
            return Err(LinuxError::ENOSYS);
        }
        if !prot.contains(MmapProt::PROT_READ)
            || prot.intersects(MmapProt::PROT_WRITE | MmapProt::PROT_EXEC)
        {
            return Err(LinuxError::ENOSYS);
        }
        if length == 0 || fd < 0 {
            return Err(LinuxError::EINVAL);
        }

        let map_size = (length + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1);
        if map_size > MMAP_MAX_SIZE {
            return Err(LinuxError::ENOMEM);
        }

        // Read the file content into a kernel buffer first. The file offset is
        // saved and restored around the read so the mapping does not disturb
        // later reads through the same fd.
        let saved_offset = api::sys_lseek(fd, 0, SEEK_CUR);
        if saved_offset < 0 {
            return Err(LinuxError::EBADF);
        }
        if api::sys_lseek(fd, offset as _, SEEK_SET) < 0 {
            return Err(LinuxError::EBADF);
        }
        let mut content = alloc::vec![0u8; length];
        let read_ret = api::sys_read(fd, content.as_mut_ptr() as *mut c_void, length);
        // Restore the file offset whether or not the read succeeded.
        let _ = api::sys_lseek(fd, saved_offset, SEEK_SET);
        if read_ret < 0 {
            return Err(LinuxError::EIO);
        }

        // Pick a free virtual address in the reserved mapping region.
        let base = VirtAddr::from_usize(MMAP_BASE);
        let hint = if addr.is_null() {
            base
        } else {
            VirtAddr::from_usize(addr as usize)
        };
        let limit = VirtAddrRange::from_start_size(base, MMAP_MAX_SIZE);

        let curr = current();
        let aspace = curr.task_ext().aspace.clone();
        let mut aspace = aspace.lock();

        let vaddr = aspace
            .find_free_area(hint, map_size, limit)
            .ok_or(LinuxError::ENOMEM)?;
        aspace
            .map_alloc(vaddr, map_size, MappingFlags::from(prot), true)
            .map_err(|_| LinuxError::ENOMEM)?;
        // Copy the file content into the mapping. The rest of the pages are
        // already zeroed by the population of the allocation backend.
        aspace
            .write(vaddr, &content)
            .map_err(|_| LinuxError::EFAULT)?;

        Ok(vaddr.as_usize())
    })
}

fn sys_openat(dfd: c_int, fname: *const c_char, flags: c_int, mode: api::ctypes::mode_t) -> isize {
    assert_eq!(dfd, AT_FDCWD);
    api::sys_open(fname, flags, mode) as isize
}

fn sys_close(fd: i32) -> isize {
    api::sys_close(fd) as isize
}

fn sys_read(fd: i32, buf: *mut c_void, count: usize) -> isize {
    api::sys_read(fd, buf, count)
}

fn sys_write(fd: i32, buf: *const c_void, count: usize) -> isize {
    api::sys_write(fd, buf, count)
}

fn sys_writev(fd: i32, iov: *const api::ctypes::iovec, iocnt: i32) -> isize {
    unsafe { api::sys_writev(fd, iov, iocnt) }
}

fn sys_set_tid_address(tid_ptd: *const i32) -> isize {
    let curr = current();
    curr.task_ext().set_clear_child_tid(tid_ptd as _);
    curr.id().as_u64() as isize
}

fn sys_ioctl(_fd: i32, _op: usize, _argp: *mut c_void) -> i32 {
    ax_println!("Ignore SYS_IOCTL");
    0
}
