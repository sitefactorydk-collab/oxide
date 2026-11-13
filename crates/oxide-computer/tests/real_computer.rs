//! The real screen and mouse (Windows). Ignored by default; `scripts\win\phase16.ps1` runs it:
//! `cargo test -p oxide-computer --test real_computer -- --ignored`.
//! It moves the mouse pointer and puts it back; it never clicks or types.

#![cfg(windows)]

use oxide_computer::Desktop;

#[test]
#[ignore = "needs a real Windows desktop"]
fn screens_capture_pointer_and_apps() {
    let d = Desktop::new();
    assert!(Desktop::supported());
    let screens = d.displays().expect("screens");
    println!("screens: {screens:?}");
    let main = &screens[0];
    assert!(main.width > 0 && main.height > 0);

    let img = d.capture(main).expect("capture");
    println!("captured {}×{}", img.width(), img.height());
    assert!(img.width() >= main.width / 2, "captured the whole screen");
    // Not one flat colour (a blank capture).
    let first = *img.get_pixel(0, 0);
    let varied = img.pixels().step_by(997).any(|p| *p != first);
    assert!(varied, "the screenshot isn't blank");

    let before = d.cursor().expect("cursor");
    let target = (
        main.x + main.width as i32 / 2,
        main.y + main.height as i32 / 2,
    );
    d.move_to(target.0, target.1).expect("moves");
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(d.cursor().expect("cursor"), target);
    // Every other screen's centre too (physical pixels across screens).
    for s in &screens[1..] {
        let p = (s.x + s.width as i32 / 2, s.y + s.height as i32 / 2);
        d.move_to(p.0, p.1).expect("moves");
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(d.cursor().expect("cursor"), p, "on {}", s.name);
    }
    let at = d.app_at(target.0, target.1);
    println!("app at the centre: {at:?}");
    d.move_to(before.0, before.1).expect("moves back");

    let front = d.foreground_app();
    println!("app in front: {front:?}");
    let front = front.expect("an app is in front");
    assert!(front.pid > 0);
    assert!(front.exe.to_lowercase().ends_with(".exe"), "{}", front.exe);
    println!("focused control: {:?}", d.focused_field());
    println!(
        "control at the centre: {:?}",
        d.element_at(target.0, target.1)
    );
}
