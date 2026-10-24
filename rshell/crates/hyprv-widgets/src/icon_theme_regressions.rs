use super::*;
fn cache() -> Vec<u8> {
    let mut d = vec![0; 80];
    d[1] = 1;
    for (o, v) in [
        (4, 12u32),
        (8, 56),
        (12, 1),
        (16, 24),
        (24, 24),
        (28, 48),
        (56, 1),
        (60, 64),
    ] {
        d[o..o + 4].copy_from_slice(&v.to_be_bytes());
    }
    d[48..54].copy_from_slice(b"other\0");
    d[64..75].copy_from_slice(b"32x32/apps\0");
    d
}
#[test]
fn cyclic_and_out_of_range_chains_are_invalid_not_empty() {
    let mut d = cache();
    assert_eq!(gtk_cache_lookup(&d, "victim"), None);
    d[24..28].copy_from_slice(&36u32.to_be_bytes());
    d[36..40].copy_from_slice(&24u32.to_be_bytes());
    d[40..44].copy_from_slice(&48u32.to_be_bytes());
    assert_eq!(gtk_cache_lookup(&d, "victim"), None);
    d[16..20].copy_from_slice(&4096u32.to_be_bytes());
    assert_eq!(gtk_cache_lookup(&d, "victim"), None);
    d[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(gtk_cache_lookup(&d, "victim"), Some(vec![]));
    d[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(gtk_cache_lookup(&d, "victim"), None);
}
#[test]
fn chain_work_is_capped_even_without_a_cycle() {
    let mut d = vec![0; 32 + (MAX_CACHE_CHAIN + 1) * 12];
    for (o, v) in [(4, 12u32), (12, 1), (16, 32)] {
        d[o..o + 4].copy_from_slice(&v.to_be_bytes());
    }
    d[24..30].copy_from_slice(b"other\0");
    for i in 0..=MAX_CACHE_CHAIN {
        let p = 32 + i * 12;
        let next = if i == MAX_CACHE_CHAIN {
            u32::MAX
        } else {
            (p + 12) as u32
        };
        d[p..p + 4].copy_from_slice(&next.to_be_bytes());
        d[p + 4..p + 8].copy_from_slice(&24u32.to_be_bytes());
    }
    assert_eq!(gtk_cache_lookup(&d, "victim"), None);
}
#[test]
fn corrupt_cache_falls_back_to_existing_icon() {
    let root = std::env::temp_dir().join(format!(
        "hyprv-icon-corrupt-regression-{}",
        std::process::id()
    ));
    let theme = root.join("hicolor");
    std::fs::create_dir_all(theme.join("32x32/apps")).unwrap();
    std::fs::write(
        theme.join("index.theme"),
        "[Icon Theme]\nName=Test\n[32x32/apps]\nSize=32\n",
    )
    .unwrap();
    std::fs::write(theme.join("32x32/apps/victim.png"), b"x").unwrap();
    let mut d = cache();
    d[16..20].copy_from_slice(&4096u32.to_be_bytes());
    std::fs::write(theme.join("icon-theme.cache"), d).unwrap();
    assert!(gtk_cache(&theme).is_some());
    let entries = lookup_entries("victim", "hicolor", std::slice::from_ref(&root), &[]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file, theme.join("32x32/apps/victim.png"));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn file_reader_bounds_regular_files_and_never_opens_a_fifo_blocking() {
    let root = std::env::temp_dir().join(format!(
        "hyprv-icon-reader-regression-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let p = root.join("file");
    std::fs::write(&p, b"12345").unwrap();
    assert_eq!(read_bounded_file(&p, 5), Some(b"12345".to_vec()));
    assert_eq!(read_bounded_file(&p, 4), None);
    assert_eq!(read_bounded_file(&root, 16), None);
    let fifo = root.join("fifo");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let (tx, rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || tx.send(read_bounded_file(&fifo, 16)).unwrap());
    assert_eq!(
        rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap(),
        None
    );
    thread.join().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
