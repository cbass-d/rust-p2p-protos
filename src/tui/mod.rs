pub mod app;
mod components;

use std::{
    io::{self, Stdout, stdout},
    time::Duration,
};

use color_eyre::Result;
use crossterm::event::KeyEventKind;
use futures::{FutureExt, StreamExt};
use ratatui::{
    Terminal,
    crossterm::{
        self, ExecutableCommand, cursor,
        event::{
            DisableMouseCapture, EnableMouseCapture, Event as CrosstermEvent, KeyEvent, MouseEvent,
        },
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode},
    },
    prelude::CrosstermBackend,
    restore,
};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

// Events originating from the user interacting with
// the TUI
pub enum TuiEvent {
    Init,
    Quit,
    Error,
    Closed,
    Tick,
    Render,
    Key(KeyEvent),
    Mouse(MouseEvent),
}

pub struct Tui {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
    pub task: JoinHandle<()>,
    pub cancellation_token: CancellationToken,

    // This mpsc channel is exclusive for TUI/Crossterm events
    pub tui_event_rx: UnboundedReceiver<TuiEvent>,
    pub tui_event_tx: UnboundedSender<TuiEvent>,

    pub frame_rate: f64,
    pub tick_rate: f64,
    pub mouse: bool,
}

impl Tui {
    pub fn new() -> Result<Self> {
        let tick_rate = 4.0;
        let frame_rate = 60.0;
        let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        let (tui_event_tx, tui_event_rx) = mpsc::unbounded_channel();
        let cancellation_token = CancellationToken::new();
        let task = tokio::spawn(async {});
        let mouse = false;

        set_panic_hook();

        Ok(Self {
            terminal,
            task,
            cancellation_token,
            tui_event_rx,
            tui_event_tx,
            frame_rate,
            tick_rate,
            mouse,
        })
    }

    pub fn frame_rate(mut self, frame_rate: f64) -> Self {
        self.frame_rate = frame_rate;
        self
    }

    pub fn tick_rate(mut self, tick_rate: f64) -> Self {
        self.tick_rate = tick_rate;
        self
    }

    pub fn mouse(mut self, mouse: bool) -> Self {
        self.mouse = mouse;
        self
    }

    pub fn start(&mut self) {
        let tick_delay = Duration::from_secs_f64(1.0 / self.tick_rate);
        let render_delay = Duration::from_secs_f64(1.0 / self.frame_rate);

        // In case the TUI application has been stopped and resume we need to create
        // a new cancellation token
        self.cancel();
        self.cancellation_token = CancellationToken::new();

        let _cancellation_token = self.cancellation_token.clone();
        let _event_tx = self.tui_event_tx.clone();

        self.task = tokio::spawn(async move {
            // Reader exclusively for Crossterm events
            let mut reader = crossterm::event::EventStream::new();

            let mut tick_interval = tokio::time::interval(tick_delay);
            let mut render_interval = tokio::time::interval(render_delay);

            // Send Init event for TUI application
            _event_tx.send(TuiEvent::Init).unwrap();

            // Handles and sends crossterm events as well as tick and render ticks
            // to TUI application
            loop {
                let tick_delay = tick_interval.tick();
                let render_delay = render_interval.tick();
                let crossterm_event = reader.next().fuse();

                tokio::select! {
                    _ = _cancellation_token.cancelled() => {
                        break;
                    }

                    maybe_event = crossterm_event => {
                        match maybe_event {
                            Some(Ok(evt)) => {
                                match evt {
                                    CrosstermEvent::Key(key) => {
                                        if key.kind == KeyEventKind::Press {
                                            _event_tx.send(TuiEvent::Key(key)).unwrap();
                                        }
                                    }
                                    CrosstermEvent::Mouse(mouse) => {
                                        _event_tx.send(TuiEvent::Mouse(mouse)).unwrap();
                                    }
                                    _ => {},
                                }

                            }
                            Some(Err(_)) => {
                                _event_tx.send(TuiEvent::Error).unwrap();
                            }
                            None => {},
                        }
                    }
                    _ = tick_delay => {
                        _event_tx.send(TuiEvent::Tick).unwrap();
                    },
                    _ = render_delay => {
                        _event_tx.send(TuiEvent::Render).unwrap();
                    },
                }
            }
        });
    }

    pub fn stop(&self) -> Result<()> {
        self.cancel();
        let mut counter = 0;

        // Make sure the task is fully finished
        while !self.task.is_finished() {
            std::thread::sleep(Duration::from_millis(1));
            counter += 1;
            if counter > 50 {
                self.task.abort();
            }
            if counter > 100 {
                break;
            }
        }

        Ok(())
    }

    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    pub fn enter(&mut self) -> Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;
        if self.mouse {
            crossterm::execute!(stdout(), EnableMouseCapture)?;
        }
        self.start();
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        self.enter()?;
        Ok(())
    }

    pub fn suspend(&mut self) -> Result<()> {
        self.exit()?;
        #[cfg(not(Windows))]
        signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
        Ok(())
    }

    pub async fn next(&mut self) -> Option<TuiEvent> {
        self.tui_event_rx.recv().await
    }

    pub fn exit(&mut self) -> Result<()> {
        self.stop()?;

        // Clean up the terminal environment
        if crossterm::terminal::is_raw_mode_enabled()? {
            if self.mouse {
                crossterm::execute!(stdout(), DisableMouseCapture)?;
            }
            crossterm::execute!(stdout(), LeaveAlternateScreen, cursor::Show)?;
            crossterm::terminal::disable_raw_mode()?;
        }

        Ok(())
    }
}

/// Set a panic hook to restore terminal if app
/// panics at runtime
pub fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore();
        hook(panic_info);
    }));
}
