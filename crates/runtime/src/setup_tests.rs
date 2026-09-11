use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "balatro-setup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn finds_owned_game_without_renaming_it() {
    let directory = Directory::new();
    assert!(find_game(&directory.0).is_err());
    for name in ["Balatro", "Balatro.love", "Balatro.exe"] {
        let path = directory.0.join(name);
        std::fs::write(&path, b"unchanged").unwrap();
        assert_eq!(find_game(&directory.0).unwrap(), path);
        assert_eq!(std::fs::read(path).unwrap(), b"unchanged");
    }
}

#[test]
fn accepts_mixed_case_but_not_a_directory_named_like_the_game() {
    let directory = Directory::new();
    std::fs::create_dir(directory.0.join("Balatro.exe")).unwrap();
    let path = directory.0.join("BALATRO.LOVE");
    std::fs::write(&path, b"game").unwrap();
    assert_eq!(find_game(&directory.0).unwrap(), path);
}

#[test]
fn error_lines_fit_and_keep_unicode_intact() {
    assert_eq!(wrap("one two three", 7), ["one two", "three"]);
    assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(wrap("ééééé", 3), ["ééé", "éé"]);
    let lines = wrap(
        "Copy Balatro.exe or Balatro.love to Roms/PORTS/Games/Balatro, then launch again.",
        36,
    );
    assert!(lines.len() <= 4);
    assert!(lines.iter().all(|line| line.chars().count() <= 36));
}

#[test]
fn rejects_invalid_game_archives() {
    let directory = Directory::new();
    let path = directory.0.join("Balatro.exe");
    std::fs::write(&path, b"not a game").unwrap();
    assert!(validate_game(&path).is_err());
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn validates_owned_game_archive() {
    let game = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    assert!(validate_game(Path::new(&game))
        .unwrap()
        .starts_with("1.0.1"));
}
