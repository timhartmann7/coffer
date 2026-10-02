//! Which disk a folder is on, and which other disks this Mac has mounted.
//!
//! A copy on the disk the vault is on goes with it when that disk fails, which
//! is the one thing a copy elsewhere is for, so the panel looks for another disk
//! and the copy says when it did not land on one. A volume's disk is the
//! physical one IOKit says it is stored on, however many partitions, APFS
//! containers and volumes lie between: the Mac's own SSD is one disk whether a
//! folder is on its data volume, in a second container or on a partition of its
//! own. A share is a disk of its own, by its address. A disk image is a file on
//! some disk Coffer does not follow it to, so it is never said to be another
//! disk, and neither is anything IOKit will not answer for.

use std::ffi::{CStr, CString, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// Where the Finder mounts every disk it shows by name.
const VOLUMES: &str = "/Volumes";

/// How many rows past the count the table is read into. A disk mounted
/// between counting and reading is left out rather than written past the end:
/// the kernel fills only what the buffer holds.
const SPARE: usize = 4;

/// How far up IOKit's service plane a volume is followed to the device it is
/// stored on. An APFS volume is some twenty steps below the registry's root;
/// a walk that has not reached it by this many has lost its way, and the
/// volume is not placed.
const DEPTH: usize = 64;

/// The drivers a mounted disk image hangs from: the one `hdiutil` has used for
/// years, and the one that has been replacing it.
const IMAGES: [&CStr; 2] = [c"IOHDIXHDDrive", c"AppleDiskImageDevice"];

/// The disk a volume is stored on, as far as Coffer can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Disk {
    /// A physical disk, by IOKit's number for the whole device.
    Physical(u64),
    /// Not a device at all, by what it is mounted from: a share's address,
    /// `devfs`.
    Named(Vec<u8>),
    /// A disk image, or a device IOKit would not place.
    Unplaced,
}

impl Disk {
    /// Whether the two are known to be different disks. Never when either
    /// cannot be placed: a copy is only said to be on another disk when Coffer
    /// knows it is.
    fn apart(&self, other: &Disk) -> bool {
        *self != Disk::Unplaced && *other != Disk::Unplaced && self != other
    }
}

/// One row of the mount table.
struct Mount {
    /// Where it is mounted.
    on: PathBuf,
    flags: u32,
    disk: Disk,
}

impl Mount {
    fn of(found: &libc::statfs) -> Mount {
        Mount {
            on: PathBuf::from(OsString::from_vec(until_nul(&found.f_mntonname))),
            flags: found.f_flags,
            disk: stored_on(&until_nul(&found.f_mntfromname)),
        }
    }

    /// The name the Finder shows it by: the folder it is mounted on, when that
    /// is in `/Volumes` itself. The disk the Mac started from is mounted at
    /// `/` and its data at `/System/Volumes/Data`, and has no such name.
    fn name(&self) -> Option<String> {
        (self.on.parent() == Some(Path::new(VOLUMES)))
            .then(|| self.on.file_name())
            .flatten()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

/// A name the kernel wrote into a fixed buffer, up to its first NUL, or the
/// whole buffer when the name filled it.
fn until_nul(buffer: &[libc::c_char]) -> Vec<u8> {
    buffer
        .iter()
        .map(|character| character.cast_unsigned())
        .take_while(|&byte| byte != 0)
        .collect()
}

/// The BSD name of the device a volume is mounted from - `disk3s5` for
/// `/dev/disk3s5` - or nothing when it is not mounted from a disk: a share,
/// `devfs`, an automounter's map.
fn device(source: &[u8]) -> Option<&[u8]> {
    let name = source.strip_prefix(b"/dev/")?;
    name.strip_prefix(b"disk")?
        .first()
        .is_some_and(u8::is_ascii_digit)
        .then_some(name)
}

/// The disk a volume mounted from `source` is stored on.
fn stored_on(source: &[u8]) -> Disk {
    match device(source) {
        Some(name) => iokit::placed(name),
        None => Disk::Named(source.to_vec()),
    }
}

/// The volume a path is on, or nothing when the path cannot be asked about.
/// `statfs` follows links, so a folder reached through one is asked about
/// where it is.
fn mounted(path: &Path) -> Option<Mount> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: a `statfs` is integers and arrays of them, for which all zeroes
    // is a value.
    let mut found: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: the name is terminated and outlives the call, and the buffer is
    // the one `statfs` the call writes.
    if unsafe { libc::statfs(path.as_ptr(), &mut found) } != 0 {
        return None;
    }
    Some(Mount::of(&found))
}

/// Everything mounted, as the kernel last knew it. `MNT_NOWAIT`: a share whose
/// server has gone is answered from the cache, rather than holding the panel
/// up for the network's timeout.
fn table() -> Vec<Mount> {
    // SAFETY: no buffer and no size asks only how many there are.
    let counted = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    let Ok(counted) = usize::try_from(counted) else {
        return Vec::new();
    };

    // SAFETY: as in `mounted`, all zeroes is a `statfs`.
    let blank: libc::statfs = unsafe { std::mem::zeroed() };
    let mut found = vec![blank; counted.saturating_add(SPARE)];
    let Ok(size) = libc::c_int::try_from(std::mem::size_of_val(found.as_slice())) else {
        return Vec::new();
    };
    // SAFETY: the buffer holds `size` bytes of `statfs`, and the call writes
    // whole rows into it and no more than that.
    let filled = unsafe { libc::getfsstat(found.as_mut_ptr(), size, libc::MNT_NOWAIT) };
    let Ok(filled) = usize::try_from(filled) else {
        return Vec::new();
    };
    found.truncate(filled);
    found.iter().map(Mount::of).collect()
}

/// Whether two folders are on one disk. A folder Coffer cannot ask about, or
/// one on a disk it cannot place, is counted as on the other's: a copy is only
/// ever said to be on another disk when Coffer knows it is.
pub fn same(one: &Path, other: &Path) -> bool {
    match (mounted(one), mounted(other)) {
        (Some(one), Some(other)) => !one.disk.apart(&other.disk),
        _ => true,
    }
}

/// The name the Finder shows for the disk a folder is on, when that disk is
/// mounted in `/Volumes`. The disk the Mac started from is not, and has none
/// here.
pub fn volume(folder: &Path) -> Option<String> {
    mounted(folder)?.name()
}

/// The other disks a copy could go to: see [`offered_of`]. None when the folder
/// `beside` cannot be asked about, since then no disk is known to be another.
pub fn others(beside: &Path) -> Vec<PathBuf> {
    let Some(here) = mounted(beside) else {
        return Vec::new();
    };
    offered_of(table(), &here.disk)
}

/// The rows of the mount table a copy could go to, by where they are mounted,
/// in order of name so that the panel opens on the same one each time. Apart
/// from [`others`] so that the suite can hand it a table no runner has.
fn offered_of(rows: Vec<Mount>, here: &Disk) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = rows
        .into_iter()
        .filter(|mount| offered(mount, here))
        .map(|mount| mount.on)
        .collect();
    found.sort();
    found
}

/// Whether one row of the table is a disk a copy could go to: mounted in
/// `/Volumes`, shown in the Finder, taking writes, and known to be another disk
/// than `here`, the one the vault is on. Coffer's own disk image is mounted
/// read only, a Time Machine disk is kept out of the Finder, and a writable
/// disk image is a file on a disk Coffer does not follow it to.
fn offered(mount: &Mount, here: &Disk) -> bool {
    let refusing = (libc::MNT_RDONLY | libc::MNT_DONTBROWSE).cast_unsigned();
    mount.name().is_some() && mount.flags & refusing == 0 && mount.disk.apart(here)
}

/// Asking IOKit where a BSD disk is stored.
mod iokit {
    use std::ffi::{CStr, CString, c_char, c_void};

    use libc::{KERN_SUCCESS, boolean_t, kern_return_t, mach_port_t};

    use super::{DEPTH, Disk, IMAGES};

    /// `kIOMainPortDefault`, which IOKit spells as no port at all.
    const DEFAULT_PORT: mach_port_t = 0;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOBSDNameMatching(
            main_port: mach_port_t,
            options: u32,
            bsd_name: *const c_char,
        ) -> *mut c_void;
        fn IOServiceGetMatchingService(
            main_port: mach_port_t,
            matching: *mut c_void,
        ) -> mach_port_t;
        fn IORegistryEntryGetParentEntry(
            entry: mach_port_t,
            plane: *const c_char,
            parent: *mut mach_port_t,
        ) -> kern_return_t;
        fn IORegistryEntryGetRegistryEntryID(entry: mach_port_t, id: *mut u64) -> kern_return_t;
        fn IOObjectConformsTo(object: mach_port_t, class_name: *const c_char) -> boolean_t;
        fn IOObjectRelease(object: mach_port_t) -> kern_return_t;
    }

    /// The disk a BSD device is stored on: the topmost `IOMedia` above it in
    /// the service plane, which is the whole physical device - the SSD under an
    /// APFS container and its partition, the stick under its own. Unplaced when
    /// a disk image's driver is on the way up, when IOKit does not know the
    /// name, and when the walk does not reach the top.
    pub(super) fn placed(device: &[u8]) -> Disk {
        let Ok(name) = CString::new(device) else {
            return Disk::Unplaced;
        };
        // SAFETY: the name is terminated and outlives the call.
        let matching = unsafe { IOBSDNameMatching(DEFAULT_PORT, 0, name.as_ptr()) };
        if matching.is_null() {
            return Disk::Unplaced;
        }
        // SAFETY: the call takes the dictionary over, whatever it finds, and
        // answers with an object this owns, or with none.
        let mut entry = unsafe { IOServiceGetMatchingService(DEFAULT_PORT, matching) };

        let mut whole = None;
        let mut image = false;
        for _ in 0..DEPTH {
            if entry == 0 {
                break;
            }
            if conforms(entry, c"IOMedia") {
                whole = number(entry).or(whole);
            }
            image |= IMAGES.iter().any(|driver| conforms(entry, driver));

            let mut parent = 0;
            // SAFETY: `entry` is an object this owns, the plane's name is
            // terminated, and `parent` is written only when the call answers
            // that it was.
            let found =
                unsafe { IORegistryEntryGetParentEntry(entry, c"IOService".as_ptr(), &mut parent) };
            // SAFETY: `entry` is owned here and not used again.
            unsafe { IOObjectRelease(entry) };
            entry = if found == KERN_SUCCESS { parent } else { 0 };
        }
        if entry != 0 {
            // SAFETY: owned here, and the walk that would have used it is over.
            unsafe { IOObjectRelease(entry) };
            return Disk::Unplaced;
        }

        match whole {
            Some(number) if !image => Disk::Physical(number),
            _ => Disk::Unplaced,
        }
    }

    fn conforms(entry: mach_port_t, class: &CStr) -> bool {
        // SAFETY: `entry` is an object the caller owns, and the class's name is
        // terminated.
        unsafe { IOObjectConformsTo(entry, class.as_ptr()) != 0 }
    }

    /// IOKit's number for a registry entry, which names it for as long as it
    /// is there.
    fn number(entry: mach_port_t) -> Option<u64> {
        let mut number = 0;
        // SAFETY: `entry` is an object the caller owns, and `number` is
        // written only when the call answers that it was.
        (unsafe { IORegistryEntryGetRegistryEntryID(entry, &mut number) } == KERN_SUCCESS)
            .then_some(number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(on: &str, flags: libc::c_int, disk: Disk) -> Mount {
        Mount {
            on: PathBuf::from(on),
            flags: flags.cast_unsigned(),
            disk,
        }
    }

    /// A volume is mounted from a device when its source names one, and the
    /// device is the name after `/dev/`; anything else is mounted from what
    /// it says it is. Nothing past what the name holds is read.
    #[test]
    fn a_volume_names_its_device_only_when_it_is_mounted_from_one() {
        for (source, named) in [
            ("/dev/disk3s5", "disk3s5"),
            ("/dev/disk3s1s1", "disk3s1s1"),
            ("/dev/disk12s2", "disk12s2"),
            ("/dev/disk4", "disk4"),
        ] {
            assert_eq!(
                device(source.as_bytes()),
                Some(named.as_bytes()),
                "{source}"
            );
        }
        for source in [
            "//me@nas.local/share",
            "map auto_home",
            "devfs",
            "",
            "/dev/",
            "/dev/disk",
            "/dev/diskXs1",
            "/dev/rdisk3s1",
            "disk3s1",
        ] {
            assert_eq!(device(source.as_bytes()), None, "{source}");
            assert_eq!(
                stored_on(source.as_bytes()),
                Disk::Named(source.as_bytes().to_vec()),
                "{source}"
            );
        }
    }

    /// Two disks are apart only when both are placed and they differ: a share
    /// and the SSD are, the SSD and itself are not, and a disk image is never
    /// apart from anything, another image included.
    #[test]
    fn only_two_placed_disks_that_differ_are_apart() {
        let ssd = Disk::Physical(0x1_0000_0872);
        let stick = Disk::Physical(0x1_0002_6397);
        let share = Disk::Named(b"//me@nas.local/share".to_vec());

        assert!(ssd.apart(&stick));
        assert!(ssd.apart(&share));
        assert!(share.apart(&Disk::Named(b"//me@nas.local/other".to_vec())));
        assert!(!ssd.apart(&ssd.clone()));
        assert!(!share.apart(&share.clone()));
        for placed in [&ssd, &share, &Disk::Unplaced] {
            assert!(!Disk::Unplaced.apart(placed));
            assert!(!placed.apart(&Disk::Unplaced));
        }
    }

    /// A stick is offered, and so is a share. Coffer's own disk image, mounted
    /// read only, is not; nor is a writable disk image, nor a Time Machine
    /// disk, which macOS keeps out of the Finder; nor a second container or
    /// partition of the disk the vault is on, or the share it lives on; nor
    /// anything not mounted in `/Volumes`, the system's own volumes included,
    /// or `/Volumes` itself. Beside a vault kept in a disk image, nothing is
    /// known to be another disk.
    #[test]
    fn only_another_writable_disk_the_finder_shows_is_offered() {
        let here = Disk::Physical(1);
        let stick = || Disk::Physical(5);

        assert!(offered(&row("/Volumes/Stick", 0, stick()), &here));
        assert!(offered(
            &row(
                "/Volumes/share",
                libc::MNT_NOSUID,
                Disk::Named(b"//me@nas.local/share".to_vec())
            ),
            &here
        ));

        for (what, mount) in [
            (
                "a disk image mounted read only",
                row("/Volumes/Coffer", libc::MNT_RDONLY, Disk::Unplaced),
            ),
            (
                "a writable disk image",
                row("/Volumes/Private", 0, Disk::Unplaced),
            ),
            (
                "a disk the Finder does not show",
                row("/Volumes/Backups", libc::MNT_DONTBROWSE, stick()),
            ),
            (
                "another container or partition of the vault's disk",
                row("/Volumes/Other", 0, here.clone()),
            ),
            (
                "the system's own data",
                row("/System/Volumes/Data", 0, stick()),
            ),
            ("/Volumes itself", row("/Volumes", 0, stick())),
            ("a disk mounted deeper", row("/Volumes/a/b", 0, stick())),
        ] {
            assert!(!offered(&mount, &here), "{what}");
        }

        let share = Disk::Named(b"//me@nas.local/share".to_vec());
        assert!(
            !offered(&row("/Volumes/share", 0, share.clone()), &share),
            "the share the vault is on"
        );
        assert!(
            !offered(&row("/Volumes/Stick", 0, stick()), &Disk::Unplaced),
            "a disk beside a vault kept in a disk image"
        );
    }

    /// The table as the kernel hands it over, in no order: what is offered
    /// comes back sorted by where it is mounted, and only that, so that the
    /// panel opens on the same disk each time.
    #[test]
    fn the_disks_offered_are_in_order_of_name_and_none_is_the_vaults() {
        let here = Disk::Physical(1);
        let rows = vec![
            row("/Volumes/Zed", 0, Disk::Physical(7)),
            row("/Volumes/Here", 0, here.clone()),
            row("/Volumes/Alpha", 0, Disk::Physical(5)),
            row("/Volumes/Locked", libc::MNT_RDONLY, Disk::Physical(6)),
            row("/", 0, here.clone()),
        ];
        assert_eq!(
            offered_of(rows, &here),
            vec![
                PathBuf::from("/Volumes/Alpha"),
                PathBuf::from("/Volumes/Zed")
            ]
        );
        assert_eq!(offered_of(Vec::new(), &here), Vec::<PathBuf>::new());
    }

    /// A name is read up to its NUL, a name that filled the buffer is read
    /// whole and not past it, and bytes past 127 - a name in UTF-8 - come
    /// back as they were written.
    #[test]
    fn a_name_the_kernel_wrote_is_read_to_its_end_and_no_further() {
        let mut buffer: [libc::c_char; 1024] = [0; 1024];
        for (at, byte) in "/Volumes/Clé".bytes().enumerate() {
            buffer[at] = byte.cast_signed();
        }
        buffer[20] = b'x'.cast_signed();
        assert_eq!(until_nul(&buffer), "/Volumes/Clé".as_bytes());

        let full: [libc::c_char; 1024] = [b'a'.cast_signed(); 1024];
        assert_eq!(until_nul(&full), vec![b'a'; 1024]);
        assert_eq!(until_nul(&[]), Vec::<u8>::new());
    }

    /// A disk mounted in `/Volumes` is called what it is mounted as, whatever
    /// that holds; nothing else has a name of a disk to show.
    #[test]
    fn only_a_disk_mounted_in_volumes_has_a_name_to_show() {
        for (on, named) in [
            ("/Volumes/Stick", Some("Stick")),
            ("/Volumes/My Passport 1", Some("My Passport 1")),
            ("/Volumes/a\u{202E}b", Some("a\u{202E}b")),
            ("/", None),
            ("/System/Volumes/Data", None),
            ("/Volumes", None),
            ("/Volumes/a/b", None),
            ("/private/var/folders", None),
        ] {
            assert_eq!(
                row(on, 0, Disk::Physical(5)).name().as_deref(),
                named,
                "{on}"
            );
        }
    }

    /// The kernel and IOKit answer for this Mac: the disk it started from is
    /// mounted at `/` from a device IOKit places on a physical disk, and a
    /// scratch folder is on that disk too. `/dev` is `devfs` on every Mac,
    /// which is not that disk, so a folder there is on another disk as far as
    /// Coffer can tell - the one `false` this machine is sure to answer. A
    /// second APFS container on the same SSD, where this Mac has one, is the
    /// same disk.
    #[test]
    fn the_root_and_dev_are_mounts_this_mac_can_name() {
        let scratch = tempfile::tempdir().expect("a scratch directory");

        let root = mounted(Path::new("/")).expect("the root can be asked about");
        assert_eq!(root.on, Path::new("/"));
        assert!(
            matches!(root.disk, Disk::Physical(_)),
            "the disk this Mac started from is not placed: {:?}",
            root.disk
        );
        let here = mounted(scratch.path()).expect("a scratch folder can be asked about");
        assert_eq!(
            here.disk, root.disk,
            "the scratch folder is on another disk"
        );

        let dev = mounted(Path::new("/dev")).expect("/dev can be asked about");
        assert_eq!(dev.disk, Disk::Named(b"devfs".to_vec()));
        assert!(!same(Path::new("/dev"), scratch.path()));
        assert!(!same(scratch.path(), Path::new("/dev")));
        assert!(same(Path::new("/"), scratch.path()));

        for container in [
            "/System/Volumes/iSCPreboot",
            "/System/Volumes/xarts",
            "/System/Volumes/Hardware",
        ] {
            let container = Path::new(container);
            if mounted(container).is_some_and(|mount| mount.on == container) {
                assert!(same(container, scratch.path()), "{}", container.display());
            }
        }
    }

    /// Two folders on this Mac's own disk are on one disk, the one the home
    /// folder is on included. And neither is in `/Volumes`, so neither has a
    /// name to show.
    #[test]
    fn two_folders_on_this_mac_are_on_one_disk() {
        let one = tempfile::tempdir().expect("a scratch directory");
        let two = tempfile::tempdir().expect("a second scratch directory");

        assert!(same(one.path(), two.path()));
        if let Some(home) = std::env::var_os("HOME") {
            assert!(same(one.path(), Path::new(&home)));
        }
        assert_eq!(volume(one.path()), None);
    }

    /// What cannot be asked about is never called another disk, and has no
    /// name to show; a device IOKit does not have is placed nowhere.
    #[test]
    fn a_folder_coffer_cannot_ask_about_is_never_another_disk() {
        let one = tempfile::tempdir().expect("a scratch directory");
        let missing = one.path().join("not there");
        let unusable = PathBuf::from(OsString::from_vec(b"/tmp/a\0b".to_vec()));

        for path in [&missing, &unusable, &PathBuf::new()] {
            assert!(same(one.path(), path), "{}", path.display());
            assert!(same(path, one.path()), "{}", path.display());
            assert_eq!(volume(path), None, "{}", path.display());
            assert_eq!(others(path), Vec::<PathBuf>::new(), "{}", path.display());
        }

        assert_eq!(stored_on(b"/dev/disk999999s1"), Disk::Unplaced);
        assert_eq!(stored_on(b"/dev/disk3\0s1"), Disk::Unplaced);
    }

    /// The table and IOKit, reached through the system's own calls: the root
    /// and `/dev` are in it, and whatever else this machine has mounted, what
    /// is offered is in `/Volumes` and not on the disk a folder of this machine
    /// is on. Which rows are offered, and in what order, is `offered_of`'s,
    /// tested above on a table no runner has.
    #[test]
    fn the_mount_table_reads_and_offers_nothing_on_the_vaults_disk() {
        let beside = tempfile::tempdir().expect("a scratch directory");
        let table = table();
        assert!(
            table.iter().any(|mount| mount.on == Path::new("/")),
            "the mount table has no root"
        );
        assert!(
            table.iter().any(|mount| mount.on == Path::new("/dev")),
            "the mount table has no /dev"
        );

        for disk in others(beside.path()) {
            assert_eq!(
                disk.parent(),
                Some(Path::new(VOLUMES)),
                "{}",
                disk.display()
            );
            assert!(!same(&disk, beside.path()), "{}", disk.display());
        }
    }
}
