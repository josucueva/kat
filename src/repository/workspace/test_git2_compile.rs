use git2::IndexEntry;
pub fn make_entry(path: Vec<u8>, id: git2::Oid, mode: u32) -> IndexEntry {
    IndexEntry {
        ctime: git2::IndexTime { seconds: 0, nanoseconds: 0 },
        mtime: git2::IndexTime { seconds: 0, nanoseconds: 0 },
        dev: 0,
        ino: 0,
        mode,
        uid: 0,
        gid: 0,
        file_size: 0,
        id,
        flags: 0,
        flags_extended: 0,
        path,
    }
}
