use std::{
    error::Error,
    os::fd::AsFd,
    time::{Duration, Instant},
};
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_pointer, wl_registry, wl_seat},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};
mod keyboard {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("src/functional/virtual-keyboard-unstable-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("src/functional/virtual-keyboard-unstable-v1.xml");
}
use keyboard::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
pub struct State;
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
wayland_client::delegate_noop!(State:ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State:ignore ZwlrVirtualPointerManagerV1);
wayland_client::delegate_noop!(State:ignore ZwlrVirtualPointerV1);
wayland_client::delegate_noop!(State:ignore ZwpVirtualKeyboardManagerV1);
wayland_client::delegate_noop!(State:ignore ZwpVirtualKeyboardV1);
pub struct Native {
    connection: Connection,
    queue: wayland_client::EventQueue<State>,
    pointer: ZwlrVirtualPointerV1,
    keyboard: ZwpVirtualKeyboardV1,
    start: Instant,
    pub extent: (u32, u32),
    pub completion: std::cell::Cell<Option<&'static str>>,
}
impl Native {
    pub fn connect() -> Result<Self, Box<dyn Error>> {
        if std::env::var("HYPRV_NESTED_TEST").as_deref() != Ok("1") {
            return Err("native test input requires private compositor".into());
        }
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
        let qh = queue.handle();
        let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=9, ())?;
        let pointer_manager: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=2, ())?;
        let pointer = pointer_manager.create_virtual_pointer(Some(&seat), &qh, ());
        let manager: ZwpVirtualKeyboardManagerV1 = globals.bind(&qh, 1..=1, ())?;
        let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());
        let context = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
        let map = xkbcommon::xkb::Keymap::new_from_names(
            &context,
            "evdev",
            "pc105",
            "us",
            "",
            None,
            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or("XKB map")?;
        let text = map.get_as_string(xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1) + "\0";
        let path =
            std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("private runtime")?)
                .join("functional-keymap");
        std::fs::write(&path, &text)?;
        let file = std::fs::File::open(&path)?;
        keyboard.keymap(1, file.as_fd(), text.len() as u32);
        std::fs::remove_file(path)?;
        queue.roundtrip(&mut State)?;
        Ok(Self {
            connection,
            queue,
            pointer,
            keyboard,
            start: Instant::now(),
            extent: (3072, 1728),
            completion: std::cell::Cell::new(None),
        })
    }
    fn time(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }
    pub fn barrier(&mut self) -> Result<(), Box<dyn Error>> {
        self.connection.flush()?;
        self.queue.roundtrip(&mut State)?;
        Ok(())
    }
    pub fn motion(&mut self, x: f64, y: f64) -> Result<(), Box<dyn Error>> {
        self.completion.set(Some("Motion"));
        self.pointer.motion_absolute(
            self.time(),
            x.max(0.) as u32,
            y.max(0.) as u32,
            self.extent.0,
            self.extent.1,
        );
        self.pointer.frame();
        self.barrier()
    }
    pub fn button(&mut self, button: u32, pressed: bool) -> Result<(), Box<dyn Error>> {
        self.completion
            .set(Some(if pressed { "Press" } else { "Release" }));
        self.pointer.button(
            self.time(),
            button,
            if pressed {
                wl_pointer::ButtonState::Pressed
            } else {
                wl_pointer::ButtonState::Released
            },
        );
        self.pointer.frame();
        self.barrier()
    }
    pub fn click(&mut self, x: f64, y: f64, button: u32) -> Result<(), Box<dyn Error>> {
        // Flush motion and press together. A roundtrip between them can stretch
        // a click past hover-close deadlines on a busy compositor.
        self.pointer.motion_absolute(
            self.time(),
            x.max(0.) as u32,
            y.max(0.) as u32,
            self.extent.0,
            self.extent.1,
        );
        self.pointer.frame();
        self.pointer
            .button(self.time(), button, wl_pointer::ButtonState::Pressed);
        self.pointer.frame();
        self.connection.flush()?;
        std::thread::sleep(Duration::from_millis(12));
        self.pointer
            .button(self.time(), button, wl_pointer::ButtonState::Released);
        self.pointer.frame();
        self.completion.set(Some("Release"));
        self.barrier()
    }
    pub fn wheel(&mut self, delta: i32, horizontal: bool) -> Result<(), Box<dyn Error>> {
        self.completion.set(Some("Scroll"));
        self.pointer.axis_source(wl_pointer::AxisSource::Wheel);
        self.pointer.axis_discrete(
            self.time(),
            if horizontal {
                wl_pointer::Axis::HorizontalScroll
            } else {
                wl_pointer::Axis::VerticalScroll
            },
            f64::from(delta) * 15.,
            delta,
        );
        self.pointer.frame();
        self.barrier()
    }
    pub fn finger(&mut self, delta: f64) -> Result<(), Box<dyn Error>> {
        self.completion.set(Some("Scroll"));
        // Hyprland's virtual-pointer axis request replaces its SAxisEvent;
        // attach the source afterwards, within the same native frame, so it is
        // not reset to Wheel (VirtualPointer.cpp:55,73).
        self.pointer
            .axis(self.time(), wl_pointer::Axis::VerticalScroll, delta);
        self.pointer.axis_source(wl_pointer::AxisSource::Finger);
        self.pointer.frame();
        self.barrier()?;
        self.pointer.axis_source(wl_pointer::AxisSource::Finger);
        self.pointer
            .axis_stop(self.time(), wl_pointer::Axis::VerticalScroll);
        self.pointer.frame();
        self.barrier()
    }
    pub fn key(&mut self, code: u32) -> Result<(), Box<dyn Error>> {
        self.completion.set(Some("Key(false)"));
        self.keyboard.key(self.time(), code, 1);
        self.keyboard.key(self.time(), code, 0);
        self.barrier()
    }
    pub fn text(&mut self, text: &str) -> Result<(), Box<dyn Error>> {
        for c in text.chars() {
            let code = match c {
                'a' => 30,
                'b' => 48,
                'c' => 46,
                'd' => 32,
                'e' => 18,
                'f' => 33,
                'g' => 34,
                'h' => 35,
                'i' => 23,
                'j' => 36,
                'k' => 37,
                'l' => 38,
                'm' => 50,
                'n' => 49,
                'o' => 24,
                'p' => 25,
                'q' => 16,
                'r' => 19,
                's' => 31,
                't' => 20,
                'u' => 22,
                'v' => 47,
                'w' => 17,
                'x' => 45,
                'y' => 21,
                'z' => 44,
                '0' => 11,
                '1'..='9' => 2 + (c as u32 - '1' as u32),
                _ => return Err("unsupported test text".into()),
            };
            self.key(code)?;
        }
        Ok(())
    }
}
