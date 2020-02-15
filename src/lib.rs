extern crate winit;
#[cfg(feature="use-vulkano")]
extern crate vulkano;
#[cfg(feature="use-vulkano")]
extern crate vulkano_win;
extern crate input;
extern crate window;

use std::time::Duration;
use std::collections::VecDeque;
use std::error::Error;


#[cfg(feature="use-vulkano")]
use std::sync::Arc;

#[cfg(feature="use-vulkano")]
use vulkano::{
    swapchain::Surface,
    instance::Instance
};

use winit::{
    event_loop::EventLoop,
    window::Window as OriginalWinitWindow,
    window::WindowBuilder,
    event::Event as WinitEvent,
    event::WindowEvent,
    event::ElementState,
    event::MouseButton as WinitMouseButton,
    event::KeyboardInput,
    event::MouseScrollDelta,
    dpi::{LogicalPosition, LogicalSize},
};
use input::{
    keyboard,
    ButtonArgs,
    ButtonState,
    CloseArgs,
    Event,
    MouseButton,
    Button,
    Input,
    FileDrag,
    ResizeArgs,
    Key,
};
use window::{
    BuildFromWindowSettings,
    OpenGLWindow,
    UnsupportedGraphicsApiError,
    Window, 
    Size, 
    WindowSettings, 
    Position, 
    AdvancedWindow,
    ProcAddress,
};

#[cfg(feature="use-vulkano")]
pub use vulkano_win::required_extensions;

pub struct WinitWindow {
    // TODO: These public fields should be changed to accessors
    events_loop: Option<EventLoop<()>>,
    
    #[cfg(feature="use-vulkano")]
    surface: Arc<Surface<OriginalWinitWindow>>,
    
    /// Winit window.
    #[cfg(not(feature="use-vulkano"))]
    pub window: OriginalWinitWindow,

    title: String,
    exit_on_esc: bool,
    should_close: bool,
    automatic_close: bool,
    queued_events: VecDeque<Event>,

    // Used to fake capturing of cursor,
    // to get relative mouse events.
    is_capturing_cursor: bool,
    // Stores the last known cursor position.
    last_cursor_pos: Option<[f64; 2]>,
    // Stores relative coordinates to emit on next poll.
    mouse_relative: Option<(f64, f64)>,
    // Used to emit cursor event after enter/leave.
    cursor_pos: Option<[f64; 2]>,
    
    /// Stores list of events ready for processing.
    pub events: VecDeque<winit::event::Event<'static, ()>>,
}

impl WinitWindow {
    
    #[cfg(not(feature="use-vulkano"))]
    pub fn new(settings: &WindowSettings) -> Self {
        let events_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(settings.get_size().width, settings.get_size().height))
            .with_title(settings.get_title())
            .build(&events_loop)
            .unwrap();

        WinitWindow {
            window,
            events_loop: Some(events_loop),

            title: settings.get_title(),
            exit_on_esc: settings.get_exit_on_esc(),
            should_close: false,
            automatic_close: settings.get_automatic_close(),
            queued_events: VecDeque::new(),

            cursor_pos: None,
            is_capturing_cursor: false,
            last_cursor_pos: None,
            mouse_relative: None,

            events: VecDeque::new(),
        }
    }

    pub fn new_with_window(settings: &WindowSettings, window: OriginalWinitWindow) -> Self {
        WinitWindow {
            window: window,
            events_loop: None,

            title: settings.get_title(),
            exit_on_esc: settings.get_exit_on_esc(),
            should_close: false,
            automatic_close: settings.get_automatic_close(),
            queued_events: VecDeque::new(),

            cursor_pos: None,
            is_capturing_cursor: false,
            last_cursor_pos: None,
            mouse_relative: None,

            events: VecDeque::new(),
        }
    }

    #[cfg(not(feature="use-vulkano"))]
    pub fn get_window(&self) -> &OriginalWinitWindow {
        &self.window
    }

    /// Get the event loop to break out event handling
    pub fn events_loop(&mut self) -> EventLoop<()> { 
        self.events_loop.take().unwrap()
    }

    /// Convert an incoming winit event to Piston input.
    /// Update cursor state if necessary.
    ///
    /// The `unknown` flag is set to `true` when the event is not recognized.
    /// This is used to poll another event to make the event loop logic sound.
    /// When `unknown` is `true`, the return value is `None`.
    pub fn handle_event(&mut self, ev: &winit::event::Event<()>, unknown: &mut bool) -> Option<Input> {
        use winit::event::Event as E;
        use winit::event::WindowEvent as WE;
        use winit::event::MouseScrollDelta;
        use input::{ Key, Motion };

        match ev {
            E::WindowEvent {
                event: WE::Resized(ref size), ..
            } => {
                let draw_size = self.draw_size();
                Some(Input::Resize(ResizeArgs {
                    window_size: [size.width as f64, size.height as f64],
                    draw_size: draw_size.into(),
                }))
            },
            
            E::WindowEvent {
                event: WE::ReceivedCharacter(ref ch), ..
            } => {
                let string = match ch {
                    // Ignore control characters and return ascii for Text event (like sdl2).
                    '\u{7f}' | // Delete
                    '\u{1b}' | // Escape
                    '\u{8}'  | // Backspace
                    '\r' | '\n' | '\t' => "".to_string(),
                    _ => ch.to_string()
                };
                Some(Input::Text(string))
            },
            E::WindowEvent {
                event: WE::Focused(ref focused), ..
            } =>
                Some(Input::Focus(*focused)),
            E::WindowEvent {
                event: WE::KeyboardInput{
                    input: winit::event::KeyboardInput{
                        state: winit::event::ElementState::Pressed,
                        virtual_keycode: Some(ref key), ref scancode, ..
                    }, ..
                }, ..
            } => {
                println!("winit key press: {:?}", key);
                let piston_key = map_key(*key);
                if let (true, Key::Escape) = (self.exit_on_esc, piston_key) {
                    self.should_close = true;
                }
                Some(Input::Button(ButtonArgs {
                    state: ButtonState::Press,
                    button: Button::Keyboard(piston_key),
                    scancode: Some(*scancode as i32),
                }))
            },
            E::WindowEvent {
                 event: WE::KeyboardInput{
                     input: winit::event::KeyboardInput{
                         state: winit::event::ElementState::Released,
                         virtual_keycode: Some(ref key), ref scancode, ..
                     }, ..
                 }, ..
             } => {
                println!("winit key release: {:?}", key);
                Some(Input::Button(ButtonArgs {
                    state: ButtonState::Release,
                    button: Button::Keyboard(map_key(*key)),
                    scancode: Some(*scancode as i32),
                }))},
            E::WindowEvent {
                event: WE::Touch(winit::event::Touch { ref phase, ref location, ref id, .. }), ..
            } => {
                use winit::event::TouchPhase;
                use input::{Touch, TouchArgs};

                Some(Input::Move(Motion::Touch(TouchArgs::new(
                    0, *id as i64, [location.x, location.y], 1.0, match phase {
                        TouchPhase::Started => Touch::Start,
                        TouchPhase::Moved => Touch::Move,
                        TouchPhase::Ended => Touch::End,
                        TouchPhase::Cancelled => Touch::Cancel
                    }
                ))))
            },
            E::WindowEvent {
                event: WE::CursorMoved{ref position, ..}, ..
            } => {
                let x = position.x as f64 / self.window.scale_factor();
                let y = position.y as f64 / self.window.scale_factor();

                if let Some(pos) = self.last_cursor_pos {
                    let dx = x - pos[0];
                    let dy = y - pos[1];
                    if self.is_capturing_cursor {
                        self.last_cursor_pos = Some([x as f64, y as f64]);
                        self.fake_capture();
                        // Skip normal mouse movement and emit relative motion only.
                        return Some(Input::Move(Motion::MouseRelative([dx as f64, dy as f64])));
                    }
                    // Send relative mouse movement next time.
                    self.mouse_relative = Some((dx as f64, dy as f64));
                }

                self.last_cursor_pos = Some([x as f64, y as f64]);
                Some(Input::Move(Motion::MouseCursor([x as f64, y as f64])))
            }
            E::WindowEvent {
                event: WE::CursorEntered{..}, ..
            } => Some(Input::Cursor(true)),
            E::WindowEvent {
                event: WE::CursorLeft{..}, ..
            } => Some(Input::Cursor(false)),
            E::WindowEvent {
                event: WE::MouseWheel{delta: MouseScrollDelta::PixelDelta(ref pos), ..}, ..
            } => Some(Input::Move(Motion::MouseScroll([pos.x as f64, pos.y as f64]))),
            E::WindowEvent {
                event: WE::MouseWheel{delta: MouseScrollDelta::LineDelta(ref x, ref y), ..}, ..
            } => Some(Input::Move(Motion::MouseScroll([*x as f64, *y as f64]))),
            E::WindowEvent {
                event: WE::MouseInput{state: winit::event::ElementState::Pressed, ref button, ..}, ..
            } => Some(Input::Button(ButtonArgs {
                state: ButtonState::Press,
                button: Button::Mouse(map_mouse(*button)),
                scancode: None,
            })),
            E::WindowEvent {
                event: WE::MouseInput{state: winit::event::ElementState::Released, ref button, ..}, ..
            } => Some(Input::Button(ButtonArgs {
                state: ButtonState::Release,
                button: Button::Mouse(map_mouse(*button)),
                scancode: None,
            })),
            E::WindowEvent {
                event: WE::HoveredFile(ref path), ..
            } => Some(Input::FileDrag(FileDrag::Hover(path.clone()))),
            E::WindowEvent {
                event: WE::DroppedFile(ref path), ..
            } => Some(Input::FileDrag(FileDrag::Drop(path.clone()))),
            E::WindowEvent {
                event: WE::HoveredFileCancelled, ..
            } => Some(Input::FileDrag(FileDrag::Cancel)),
            E::WindowEvent { event: WE::CloseRequested, .. } => {
                if self.automatic_close {
                    self.should_close = true;
                }
                Some(Input::Close(CloseArgs))
            }
            _ => {
                *unknown = true;
                None
            }
        }
    }
    
    // These events are emitted before popping a new event from the queue.
    // This is because Piston handles some events separately.
    fn pre_pop_front_event(&mut self) -> Option<Input> {
        use input::Motion;

        // Check for a pending mouse cursor move event.
        if let Some(pos) = self.cursor_pos {
            self.cursor_pos = None;
            return Some(Input::Move(Motion::MouseCursor(pos)));
        }

        // Check for a pending relative mouse move event.
        if let Some((x, y)) = self.mouse_relative {
            self.mouse_relative = None;
            return Some(Input::Move(Motion::MouseRelative([x, y])));
        }

        None
    }

    fn fake_capture(&mut self) {
        if let Some(pos) = self.last_cursor_pos {
            // Fake capturing of cursor.
            let size = self.size();
            let cx = size.width / 2.0;
            let cy = size.height / 2.0;
            let dx = cx - pos[0];
            let dy = cy - pos[1];
            if dx != 0.0 || dy != 0.0 {
                if let Ok(_) = self.window.set_cursor_position(winit::dpi::PhysicalPosition{x: cx, y: cy}) {
                    self.last_cursor_pos = Some([cx, cy]);
                }
            }
        }
    }

}

impl Window for WinitWindow {
    fn set_should_close(&mut self, value: bool) {
        self.should_close = value;
    }

    fn should_close(&self) -> bool {
        self.should_close
    }

    fn size(&self) -> Size {
        let (w, h) : (i32, i32) = self.get_window().inner_size().into();
        let hidpi = self.get_window().scale_factor();
        Size{width: (w as f64 / hidpi), height: (h as f64 / hidpi)}
    }

    fn swap_buffers(&mut self) {
        /*
        // This window backend was made for use with a vulkan renderer that handles swapping by
        //  itself, if you need it here open up an issue. What we can use this for however is
        //  detecting the end of a frame, which we can use to gather up cursor_accumulator data.

        if self.capture_cursor {
            let mut center = self.get_window().inner_size();
            center.width /= 2;
            center.height /= 2;

            // Center-lock the cursor if we're using capture_cursor
            self.get_window().set_cursor_position(LogicalPosition{x: center.width as i32, y: center.height as i32}).unwrap();

            // Create a relative input based on the distance from the center
            self.queued_events.push_back(Event::Input(
                Input::Move(Motion::MouseRelative([
                    self.cursor_accumulator.x,
                    self.cursor_accumulator.y,
                ])
            ), None));

            self.cursor_accumulator = LogicalPosition::new(0.0, 0.0);
        }
        */
    }

    fn wait_event(&mut self) -> Event {
        self.poll_event().unwrap()
    }

    fn wait_event_timeout(&mut self, _timeout: Duration) -> Option<Event> {
        self.poll_event()
    }

    fn poll_event(&mut self) -> Option<Event> {
        /*
        let mut center : LogicalSize<f64> = self.get_window().inner_size().to_logical(self.get_window().scale_factor());
        center.width /= 2.;
        center.height /= 2.;

        // Add all events we got to the event queue, since winit only allows us to get all pending
        //  events at once.
        {
            //let mut events: Vec<winit::event::Event<()>> = Vec::new();
            for event in self.events.into_iter() {
                self.handle_event(event, center)
            }
            
        }

        // Get the first event in the queue
        let event = self.queued_events.pop_front();

        // Check if we got a close event, if we did we need to mark ourselves as should-close
        if let &Some(Event::Input(Input::Close(_), ..)) = &event {
            self.set_should_close(true);
        }

        event
        */
        use winit::event::Event as E;
        use winit::event::WindowEvent as WE;

        // Loop to skip unknown events.
        loop {
            let event = self.pre_pop_front_event();
            if event.is_some() {
                return event.map(|x| Event::Input(x, None));
            }

            if self.events.len() == 0 {
                /*
                let ref mut events = self.events;
                self.events_loop.run_return(move |ev, _, flow| {
                    println!("{:?}", ev);
                    match ev {
                        winit::event::Event::LoopDestroyed => {
                        },
                        _ => {
                            events.push_back(ev.to_static().unwrap()); 
                            *flow = winit::event_loop::ControlFlow::Exit;
                        }
                    }
                });
                */
                //println!("No events: {}", self.events.len());
                return None;
            }
            let mut ev = self.events.pop_front();

            if self.is_capturing_cursor &&
               self.last_cursor_pos.is_none() {
                if let Some(E::WindowEvent {
                    event: WE::CursorMoved{ position, ..}, ..
                }) = ev {
                    // Ignore this event since mouse positions
                    // should not be emitted when capturing cursor.
                    self.last_cursor_pos = Some([position.x as f64, position.y as f64]);

                    if self.events.len() == 0 {
                        return None;    
                    }
                    ev = self.events.pop_front();
                }
            }

            let mut unknown = false;
            //let event = self.handle_event(ev, &mut unknown);
            //if unknown {continue};
            //return event.map(|x| Event::Input(x, None));
        }
    }

    fn draw_size(&self) -> Size {
        let size = self.get_window().inner_size();
        (size.width, size.height).into()
    }
}

impl AdvancedWindow for WinitWindow {
    fn get_title(&self) -> String {
        self.title.clone()
    }

    fn set_title(&mut self, value: String) {
        self.get_window().set_title(&value);
        self.title = value;
    }

    fn get_exit_on_esc(&self) -> bool {
        self.exit_on_esc
    }

    fn set_exit_on_esc(&mut self, value: bool) {
        self.exit_on_esc = value
    }

    fn set_capture_cursor(&mut self, value: bool) {
        // Normally we would call `.grab_cursor(true)`
        // but since relative mouse events does not work,
        // the capturing of cursor is faked by hiding the cursor
        // and setting the position to the center of window.
        self.is_capturing_cursor = value;
        self.window.set_cursor_visible(!value);
        if value {
            self.fake_capture();
        }
    }

    fn get_automatic_close(&self) -> bool {
        false
    }

    fn set_automatic_close(&mut self, _value: bool) {
        // TODO: Implement this
    }

    fn show(&mut self) {
        self.get_window().set_visible(true); 
    }

    fn hide(&mut self) {
        self.get_window().set_visible(false);
    }

    fn get_position(&self) -> Option<Position> {
        match self.get_window().outer_position() {
            Ok(pos) => Some(Position { x: pos.x as i32, y: pos.y as i32 }),
            Err(_) => None
        }
    }

    fn set_position<P: Into<Position>>(&mut self, val: P) {
        let pos: Position = val.into();
        self.get_window().set_outer_position(LogicalPosition{x: pos.x, y: pos.y});
    }

    fn set_size<S: Into<Size>>(&mut self, size: S) {
        let size: Size = size.into();
        let hidpi = self.get_window().scale_factor();
        self.get_window().set_inner_size(LogicalSize::new(
            size.width as f64 * hidpi,
            size.height as f64 * hidpi
        ));
    }
}

impl BuildFromWindowSettings for WinitWindow {
    fn build_from_window_settings(settings: &WindowSettings) -> Result<Self, Box<Error>> {
        Ok(WinitWindow::new(settings))
    }
}
/*
impl OpenGLWindow for WinitWindow {
    fn get_proc_address(&mut self, proc_name: &str) -> ProcAddress {
        //self.ctx.get_proc_address(proc_name) as *const _
        std::ptr::null()
    }

    fn is_current(&self) -> bool {
        //self.ctx.is_current()
        true
    }

    fn make_current(&mut self) {
        //use std::mem::{replace, zeroed, forget};

        //let ctx = replace(&mut self.ctx, unsafe{zeroed()});
        //forget(replace(&mut self.ctx, unsafe {ctx.make_current().unwrap()}));
    }
}
*/

/// Maps Glutin's key to Piston's key.
pub fn map_key(keycode: winit::event::VirtualKeyCode) -> keyboard::Key {
    use input::keyboard::Key;
    use winit::event::VirtualKeyCode as K;


    let key = match keycode {
        K::Key0 => Key::D0,
        K::Key1 => Key::D1,
        K::Key2 => Key::D2,
        K::Key3 => Key::D3,
        K::Key4 => Key::D4,
        K::Key5 => Key::D5,
        K::Key6 => Key::D6,
        K::Key7 => Key::D7,
        K::Key8 => Key::D8,
        K::Key9 => Key::D9,
        K::A => Key::A,
        K::B => Key::B,
        K::C => Key::C,
        K::D => Key::D,
        K::E => Key::E,
        K::F => Key::F,
        K::G => Key::G,
        K::H => Key::H,
        K::I => Key::I,
        K::J => Key::J,
        K::K => Key::K,
        K::L => Key::L,
        K::M => Key::M,
        K::N => Key::N,
        K::O => Key::O,
        K::P => Key::P,
        K::Q => Key::Q,
        K::R => Key::R,
        K::S => Key::S,
        K::T => Key::T,
        K::U => Key::U,
        K::V => Key::V,
        K::W => Key::W,
        K::X => Key::X,
        K::Y => Key::Y,
        K::Z => Key::Z,
        K::Apostrophe => Key::Unknown,
        K::Backslash => Key::Backslash,
        K::Back => Key::Backspace,
        // K::CapsLock => Key::CapsLock,
        K::Delete => Key::Delete,
        K::Comma => Key::Comma,
        K::Down => Key::Down,
        K::End => Key::End,
        K::Return => Key::Return,
        K::Equals => Key::Equals,
        K::Escape => Key::Escape,
        K::F1 => Key::F1,
        K::F2 => Key::F2,
        K::F3 => Key::F3,
        K::F4 => Key::F4,
        K::F5 => Key::F5,
        K::F6 => Key::F6,
        K::F7 => Key::F7,
        K::F8 => Key::F8,
        K::F9 => Key::F9,
        K::F10 => Key::F10,
        K::F11 => Key::F11,
        K::F12 => Key::F12,
        K::F13 => Key::F13,
        K::F14 => Key::F14,
        K::F15 => Key::F15,
        K::F16 => Key::F16,
        K::F17 => Key::F17,
        K::F18 => Key::F18,
        K::F19 => Key::F19,
        K::F20 => Key::F20,
        K::F21 => Key::F21,
        K::F22 => Key::F22,
        K::F23 => Key::F23,
        K::F24 => Key::F24,
        // Possibly next code.
        // K::F25 => Key::Unknown,
        K::Numpad0 => Key::NumPad0,
        K::Numpad1 => Key::NumPad1,
        K::Numpad2 => Key::NumPad2,
        K::Numpad3 => Key::NumPad3,
        K::Numpad4 => Key::NumPad4,
        K::Numpad5 => Key::NumPad5,
        K::Numpad6 => Key::NumPad6,
        K::Numpad7 => Key::NumPad7,
        K::Numpad8 => Key::NumPad8,
        K::Numpad9 => Key::NumPad9,
        K::NumpadComma => Key::NumPadDecimal,
        K::Divide => Key::NumPadDivide,
        K::Multiply => Key::NumPadMultiply,
        K::Subtract => Key::NumPadMinus,
        K::Add => Key::NumPadPlus,
        K::NumpadEnter => Key::NumPadEnter,
        K::NumpadEquals => Key::NumPadEquals,
        K::LShift => Key::LShift,
        K::LControl => Key::LCtrl,
        K::LAlt => Key::LAlt,
        K::RShift => Key::RShift,
        K::RControl => Key::RCtrl,
        K::RAlt => Key::RAlt,
        // Map to backslash?
        // K::GraveAccent => Key::Unknown,
        K::Home => Key::Home,
        K::Insert => Key::Insert,
        K::Left => Key::Left,
        K::LBracket => Key::LeftBracket,
        // K::Menu => Key::Menu,
        K::Minus => Key::Minus,
        K::Numlock => Key::NumLockClear,
        K::PageDown => Key::PageDown,
        K::PageUp => Key::PageUp,
        K::Pause => Key::Pause,
        K::Period => Key::Period,
        K::Snapshot => Key::PrintScreen,
        K::Right => Key::Right,
        K::RBracket => Key::RightBracket,
        K::Scroll => Key::ScrollLock,
        K::Semicolon => Key::Semicolon,
        K::Slash => Key::Slash,
        K::Space => Key::Space,
        K::Tab => Key::Tab,
        K::Up => Key::Up,
        // K::World1 => Key::Unknown,
        // K::World2 => Key::Unknown,
        _ => Key::Unknown,
    };
    println!("Keycode press: {:?} {:?}", keycode, key);
    key
}

/// Maps Glutin's mouse button to Piston's mouse button.
pub fn map_mouse(mouse_button: winit::event::MouseButton) -> MouseButton {
    use winit::event::MouseButton as M;

    match mouse_button {
        M::Left => MouseButton::Left,
        M::Right => MouseButton::Right,
        M::Middle => MouseButton::Middle,
        M::Other(0) => MouseButton::X1,
        M::Other(1) => MouseButton::X2,
        M::Other(2) => MouseButton::Button6,
        M::Other(3) => MouseButton::Button7,
        M::Other(4) => MouseButton::Button8,
        _ => MouseButton::Unknown
    }
}
