//! Pitfall 9 byte-stability proof for `build_deterministic_tar`.

use rollout_snapshots::tar_build::build_deterministic_tar;
use std::fs;
use tempfile::tempdir;

#[test]
fn deterministic_tar_byte_stability() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("c.bin"), b"content C").unwrap();
    fs::write(tmp.path().join("a.bin"), b"content A").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub").join("b.bin"), b"content B").unwrap();

    let bytes1 = build_deterministic_tar(tmp.path()).unwrap();
    let bytes2 = build_deterministic_tar(tmp.path()).unwrap();
    assert_eq!(bytes1, bytes2, "same input must produce byte-identical tar");

    let hash1 = blake3::hash(&bytes1);
    let hash2 = blake3::hash(&bytes2);
    assert_eq!(hash1, hash2);
}

#[test]
fn deterministic_tar_explicit_mode_bits() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), b"hi").unwrap();
    let bytes = build_deterministic_tar(tmp.path()).unwrap();

    let mut archive = tar::Archive::new(&bytes[..]);
    let mut saw_file = false;
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        let header = entry.header();
        if header.entry_type() == tar::EntryType::Regular {
            assert_eq!(
                header.mode().unwrap(),
                0o644,
                "regular file mode must be 0o644 explicitly (Pitfall 9)"
            );
            assert_eq!(header.mtime().unwrap(), 0);
            assert_eq!(header.uid().unwrap(), 0);
            assert_eq!(header.gid().unwrap(), 0);
            saw_file = true;
        }
    }
    assert!(
        saw_file,
        "test setup should have produced at least one regular file"
    );
}

#[test]
fn deterministic_tar_empty_dir() {
    let tmp = tempdir().unwrap();
    let bytes = build_deterministic_tar(tmp.path()).unwrap();
    assert!(
        !bytes.is_empty(),
        "tar should at minimum have terminator blocks"
    );
    let bytes2 = build_deterministic_tar(tmp.path()).unwrap();
    assert_eq!(bytes, bytes2);
}

#[test]
fn deterministic_tar_round_trip_via_extract() {
    use rollout_snapshots::tar_build::extract_tar;
    let src = tempdir().unwrap();
    fs::write(src.path().join("a.txt"), b"AAA").unwrap();
    fs::create_dir(src.path().join("nested")).unwrap();
    fs::write(src.path().join("nested/b.txt"), b"BBB").unwrap();

    let bytes = build_deterministic_tar(src.path()).unwrap();

    let dst = tempdir().unwrap();
    extract_tar(&bytes, dst.path()).unwrap();

    assert_eq!(fs::read(dst.path().join("a.txt")).unwrap(), b"AAA");
    assert_eq!(fs::read(dst.path().join("nested/b.txt")).unwrap(), b"BBB");
}
