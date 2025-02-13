//! This module provides functionality for creating and managing progress bars and spinners
//! in the terminal. It includes the `ProgressBar` struct for individual progress bars and
//! spinners, and the `MultiProgress` struct for managing multiple progress bars.
//! It was inspired by the `indicatif` crate, but is much simpler and more lightweight.  It also
//! is written specifically for use with the `crossterm` crate.

use crossterm::cursor::{
    MoveDown, MoveToColumn, MoveToNextLine, MoveToRow, MoveUp,
};
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate, ScrollUp};
use crossterm::tty::IsTty;
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use std::io::{stdout, IsTerminal, Write};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, yield_now};

#[allow(dead_code)]
/// The spinner characters to use for the spinner progress bar
const SPINNER_CHARS: &[char] = &[
    '⠁', '⠁', '⠉', '⠙', '⠚', '⠒', '⠂', '⠂', '⠒', '⠲', '⠴', '⠤', '⠄', '⠄', '⠤', '⠠', '⠠', '⠤', '⠦',
    '⠖', '⠒', '⠐', '⠐', '⠒', '⠓', '⠋', '⠉', '⠈', '⠈',
];

/// # ProgressBarStyle
/// The `ProgressBarStyle` enum is used to specify the style of the progress bar.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ProgressBarStyle {
    /// A standard progress bar
    Bar,
    /// A spinner progress bar
    Spinner,
    /// A hidden progress bar
    Hidden,
}

/// # SharedState
/// The `SharedState` struct is used to hold shared state between `ProgressBar` and `MultiProgress`.
#[derive(Clone)]
struct SharedState {
    //in_use: bool,
}

impl SharedState {
    pub fn instance() -> &'static Mutex<Self> {
        static INSTANCE: OnceLock<Mutex<SharedState>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(SharedState {  }))
    }

    // pub fn set_in_use(&mut self, value: bool) {
    //     self.in_use = value;
    // }

    // pub fn get_in_use(&mut self) -> bool {
    //     self.in_use
    // }
}

/// # ProgressBar
/// The `ProgressBar` struct is used to create and manage individual progress bars and spinners.
#[derive(Clone)]
pub struct ProgressBar {
    /// # style
    /// The style of the progress bar
    style: ProgressBarStyle,
    /// # progress
    /// The current progress of the progress bar
    progress: Arc<Mutex<u32>>,
    /// # total
    /// The total value of the progress bar
    total: u32,
    /// # is_spinning
    /// A boolean value indicating whether the spinner is currently spinning
    is_spinning: Arc<Mutex<bool>>,
    /// # message
    /// The message to display with the progress bar
    message: Arc<Mutex<String>>,
    /// # spinner_index
    /// The current index of the spinner character
    spinner_index: Arc<Mutex<usize>>,
    /// # is_spinner_thread_running
    /// is the spinner thread running
    is_spinner_thread_running: Arc<Mutex<bool>>,
    /// # row
    /// the row we draw ourselves on. Defaults to current row at init
    row: Arc<Mutex<u16>>,
    shared_state: &'static Mutex<SharedState>,
}

#[allow(dead_code)]
impl ProgressBar {
    /// # new
    /// Creates a new `ProgressBar` with the specified total value.
    /// ## Parameters
    /// - `total`: The total value of the progress bar
    /// ## Returns
    /// A new `ProgressBar` instance
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// ```
    pub fn new(total: u32) -> Self {
        ProgressBar {
            style: ProgressBarStyle::Bar,
            progress: Arc::new(Mutex::new(0)),
            total,
            is_spinning: Arc::new(Mutex::new(false)),
            message: Arc::new(Mutex::new(String::new())),
            spinner_index: Arc::new(Mutex::new(0)),
            is_spinner_thread_running: Arc::new(Mutex::new(false)),
            row: Arc::new(Mutex::new(if stdout().is_terminal() {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
            shared_state: SharedState::instance(),
        }
    }

    /// # new_spinner
    /// Creates a new `ProgressBar` with a spinner style.
    /// ## Returns
    /// A new `ProgressBar` instance with a spinner style
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let spinner = ProgressBar::new_spinner();
    /// ```
    pub fn new_spinner() -> Self {
        let progress_bar = ProgressBar {
            style: ProgressBarStyle::Spinner,
            progress: Arc::new(Mutex::new(0)),
            total: 1, // Spinner doesn't need a total value
            is_spinning: Arc::new(Mutex::new(false)),
            message: Arc::new(Mutex::new(String::new())),
            spinner_index: Arc::new(Mutex::new(0)),
            is_spinner_thread_running: Arc::new(Mutex::new(false)),
            row: Arc::new(Mutex::new(if stdout().is_terminal() {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
            shared_state: SharedState::instance(),
        };
        progress_bar
    }

    /// # hidden
    /// Creates a new `ProgressBar` with a hidden style.
    /// ## Returns
    /// A new `ProgressBar` instance with a hidden style
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let hidden = ProgressBar::hidden();
    /// ```
    pub fn hidden() -> Self {
        ProgressBar {
            style: ProgressBarStyle::Hidden,
            progress: Arc::new(Mutex::new(0)),
            total: 1,
            is_spinning: Arc::new(Mutex::new(false)),
            message: Arc::new(Mutex::new(String::new())),
            spinner_index: Arc::new(Mutex::new(0)),
            is_spinner_thread_running: Arc::new(Mutex::new(false)),
            row: Arc::new(Mutex::new(if stdout().is_terminal() {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
            shared_state: SharedState::instance(),
        }
    }

    /// # with_message
    /// Sets the message to display with the progress bar.
    /// ## Parameters
    /// - `msg`: The message to display
    /// ## Returns
    /// The `ProgressBar` instance with the message set
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100).with_message("Loading...");
    /// ```
    pub fn with_message(self, msg: &str) -> Self {
        self.set_message(msg);
        self
    }

    /// # get_style
    /// Gets the style of the progress bar.
    /// ## Returns
    /// The style of the progress bar
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// let style = progress_bar.get_style();
    /// ```
    pub fn get_style(&self) -> ProgressBarStyle {
        self.style
    }

    /// # increment
    /// Increments the progress of the progress bar by the specified value.
    /// ## Parameters
    /// - `value`: The value to increment the progress by
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.increment(10);
    /// ```
    pub fn increment(&self, value: u32) {
        if !self.style.eq(&ProgressBarStyle::Spinner) {
            let mut progress = self.progress.lock().unwrap();
            *progress += value;
            if *progress > self.total {
                *progress = self.total;
            }
            drop(progress);
            self.draw();
        } else {
            let mut index = self.spinner_index.lock().unwrap();
            *index = (*index + 1) % SPINNER_CHARS.len();

            drop(index);
            self.draw();
        }
    }

    /// # set_position
    /// Set the progress of the progress bar by the specified value
    /// ## Parameters
    /// - `value`: The value to increment the progress by
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.set_position(10);
    /// ```
    pub fn set_position(&self, value: u32) {
        if !self.style.eq(&ProgressBarStyle::Spinner) {
            let mut progress = self.progress.lock().unwrap();
            *progress = value;
            if *progress > self.total {
                *progress = self.total;
            }
            drop(progress);
            self.draw();
        } else {
            let mut index = self.spinner_index.lock().unwrap();
            *index = (*index + value as usize) % SPINNER_CHARS.len();

            drop(index);
            self.draw();
        }
    }

    /// # get_position
    /// Gets the current position of the progress bar.
    /// ## Returns
    /// The current position value
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// let progress = progress_bar.get_progress();
    /// assert_eq!(progress, 0);
    /// ```
    pub fn get_position(&self) -> u32 {
        let progress = self.progress.lock().unwrap();
        *progress
    }

    /// # get_message
    /// Gets the message to display with the progress bar.
    /// ## Returns
    /// The message to display
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// let message = progress_bar.get_message();
    /// ```
    pub fn get_message(&self) -> String {
        let message = self.message.lock().unwrap();
        message.clone()
    }

    /// # set_message
    /// Sets the message to display with the progress bar.
    /// ## Parameters
    /// - `msg`: The message to display
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.set_message("Loading...");
    /// ```
    pub fn set_message(&self, msg: &str) {
        let mut message = self.message.lock().unwrap();
        *message = msg.to_string();
        drop(message);
        self.draw();
    }

    /// # println
    /// Prints a message to the terminal, above the progress bar. If the terminal is not a TTY,
    /// the message is printed to stdout.
    /// ## Parameters
    /// - `message`: The message to print
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.println("Loading...");
    /// ```
    pub fn println(&self, message: &str) {
        let mut stdout = stdout();
        let is_terminal = stdout.is_terminal();
        // If the terminal is a TTY, print the message above the progress bar
        if is_terminal {
            // get a lock the shared state instance
            let mut _guard = self.shared_state.lock().expect("Failed to lock shared state");
            execute!(stdout, BeginSynchronizedUpdate).unwrap();
            // if we are the bottom of the terminal, scroll up everthing above
            let (_, rows) = crossterm::terminal::size().unwrap();
            let current_row = crossterm::cursor::position().unwrap().1;

            if rows == current_row + 1 {
                queue!(stdout, ScrollUp(1)).unwrap();
                queue!(stdout, MoveUp(1)).unwrap();
            }

            queue!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine),).unwrap();

            let (_, rows) = crossterm::terminal::size().unwrap();
            let current_row = crossterm::cursor::position().unwrap().1;

            queue!(
                stdout,
                Print(format!("{} {} {}", current_row, rows, message))
            )
            .unwrap();

            queue!(stdout, MoveDown(1), MoveToColumn(0)).unwrap();
            let current_row = crossterm::cursor::position().unwrap().1;

            if self.style.eq(&ProgressBarStyle::Bar) {
                self.render_bar();
            } else if self.style.eq(&ProgressBarStyle::Spinner) {
                self.render_spinner(false, Some(current_row));
            }
            stdout.flush().unwrap();
            execute!(stdout, EndSynchronizedUpdate).unwrap();
        } else {
            // If the terminal is not a TTY, print the message to stdout
            // this is included for testing purposes where there is no TTY or for redirection to a file
            println!("{}", message);
        }
    }

    /// # eprintln
    /// Prints an error message to the terminal, above the progress bar - colored in red. If the terminal is not a TTY,
    /// the message is printed to stdout.
    /// ## Parameters
    /// - `message`: The message to print
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.eprintln("This is an error");
    /// ```
    pub fn eprintln(&self, message: &str) {
        let mut stdout = stdout();
        let is_terminal = stdout.is_terminal();
        // If the terminal is a TTY, print the message above the progress bar
        if is_terminal {
            // get a lock the shared state instance
            let mut _guard = self.shared_state.lock().unwrap();
            execute!(stdout, BeginSynchronizedUpdate).unwrap();
            // if we are the bottom of the terminal, scroll up everthing above
            let (_, rows) = crossterm::terminal::size().unwrap();
            let current_row = crossterm::cursor::position().unwrap().1;

            if rows == current_row + 1 {
                queue!(stdout, ScrollUp(1)).unwrap();
                queue!(stdout, MoveUp(1)).unwrap();
            }

            queue!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine),).unwrap();

            let (_, rows) = crossterm::terminal::size().unwrap();
            let current_row = crossterm::cursor::position().unwrap().1;

            queue!(
                stdout,
                SetForegroundColor(Color::Red),
                Print(format!("{} {} {}", current_row, rows, message)),
                ResetColor,
            )
            .unwrap();

            queue!(stdout, MoveDown(1), MoveToColumn(0)).unwrap();
            if self.style.eq(&ProgressBarStyle::Bar) {
                self.render_bar();
            } else if self.style.eq(&ProgressBarStyle::Spinner) {
                self.render_spinner(false, Some(current_row));
            }
            stdout.flush().unwrap();
            execute!(stdout, EndSynchronizedUpdate).unwrap();
        } else {
            // If the terminal is not a TTY, print the message to stdout
            // this is included for testing purposes where there is no TTY or for redirection to a file
            eprintln!("{}", message);
        }
    }

    /// # with_start_spinner
    /// Starts the spinner for the progress bar.
    /// ## Returns
    /// The `ProgressBar` instance with the spinner started
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new_spinner().with_start_spinner();
    /// ```
    pub fn with_start_spinner(self) -> Self {
        self.start_spinner();
        self
    }

    /// # start_spinner
    /// Starts the spinner for the progress bar.
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new_spinner();
    /// progress_bar.start_spinner();
    /// ```
    pub fn start_spinner(&self) {
        // ignore if not a spinner style
        if !self.style.eq(&ProgressBarStyle::Spinner) {
            return;
        }

        let is_spinning = Arc::clone(&self.is_spinning);
        let is_spinner_thread_running = Arc::clone(&self.is_spinner_thread_running);
        let s = self.clone();
        let shared_state = self.shared_state;
        *is_spinning.lock().unwrap() = true;
        *is_spinner_thread_running.lock().unwrap() = true;

        // Start a new thread to draw the spinner
        // This allows the spinner to run independently of the main thread
        // and update the spinner while the main thread is doing other work
        // This is useful for long running tasks where the spinner needs to be updated
        // while the main thread is busy
        thread::spawn(move || {
            while *is_spinner_thread_running.lock().unwrap() {
                if *is_spinning.lock().unwrap() {
                    let mut _guard = shared_state.lock().unwrap();
                    //let mut stdout = stdout();
                    s.render_spinner(true, None);
                    //stdout.flush().unwrap();
                    drop(_guard);
                }
                yield_now();
                // Check every 100ms to see if the spinner should stop
                // this allows the spinner to stop quickly when the main thread
                // sets is_spinning to false, and provides 100ms for animation of the spinner
                // for _ in 0..20 {
                //     if !*is_spinning.lock().unwrap() {
                //         break;
                //     }
                //     thread::sleep(Duration::from_millis(5));
                // }
            }
        });
    }

    /// # stop_spinner
    /// Stops the spinner for the progress bar.
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new_spinner().with_start_spinner();
    /// progress_bar.stop_spinner();
    /// ```
    pub fn stop_spinner(&self) {
        let mut is_spinning = self.is_spinning.lock().unwrap();
        *is_spinning = false;
    }

    /// # render_spinner
    /// Draws the spinner for the progress bar.
    /// ## Parameters
    /// - `inc_index`: A boolean value indicating whether to increment the spinner index
    fn render_spinner(&self, inc_index: bool, row_position: Option<u16>) {
        // ignore if not a spinner style
        if !self.style.eq(&ProgressBarStyle::Spinner) {
            return;
        }
        // only on TTY
        let mut stdout = stdout();
        let is_terminal = stdout.is_terminal();
        if is_terminal {
            // get a lock the shared state instance
            //let mut _guard = self.shared_state.lock().unwrap();

            let message = Arc::clone(&self.message);
            let mut index = self.spinner_index.lock().unwrap();

            // if a row position was passed, then use it
            let mut row = self.row.lock().unwrap();
            if let Some(r) = row_position {
                execute!(stdout, MoveToRow(r)).unwrap();
                // Spock: "Remember"
                *row = r;
            } else {
                execute!(stdout, MoveToRow(*row)).unwrap();
            }

            execute!(
                stdout,
                MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                Print(format!(
                    "{} {}",
                    SPINNER_CHARS[*index],
                    *message.lock().unwrap()
                )),
            )
            .unwrap();

            if inc_index {
                *index = (*index + 1) % SPINNER_CHARS.len();
            }
        }
    }

    /// # draw
    /// Draws the progress bar to the terminal.
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.draw();
    /// ```
    pub fn draw(&self) {
        // ignore if no TTY
        let stdout = stdout();
        let is_terminal = stdout.is_terminal();
        if !is_terminal {
            return;
        } else if self.style.eq(&ProgressBarStyle::Hidden) {
            return;
        } else if self.style.eq(&ProgressBarStyle::Spinner) {
            let is_spinning = self.is_spinning.lock().unwrap();

            self.render_spinner(false, None);
            //stdout.flush().unwrap();
            drop(is_spinning);
        } else {
            self.render_bar();
            //stdout.flush().unwrap();
        }
    }

    /// # draw_bar
    /// Draws the progress bar to the terminal but does not flush
    pub fn render_bar(&self) {
        let progress = self.get_position();
        let percentage = (progress as f64 / self.total as f64) * 100.0;
        let message = self.message.lock().unwrap();

        let mut stdout = stdout();

        execute!(
            stdout,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            Print(format!(
                "[{}{}] [{}/{}] {}",
                "=".repeat((percentage / 2.0) as usize),
                " ".repeat(50 - (percentage / 2.0) as usize),
                progress,
                self.total,
                *message
            ))
        )
        .unwrap();
    }

    /// # finish
    /// Finishes the progress bar or spinner. For spinners, this stops the spinner.
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.finish();
    /// ```
    pub fn finish(&self) {
        if self.style.eq(&ProgressBarStyle::Spinner) {
            let mut is_spinner_thread_running = self.is_spinner_thread_running.lock().unwrap();
            *is_spinner_thread_running = false;
        }
    }

    pub fn finish_and_clear(&self) {}
}

/// # MultiProgress
/// The `MultiProgress` struct is used to manage multiple progress bars and spinners.
/// It provides functionality for adding, removing, and updating progress bars, as well as
/// printing messages to the terminal.
#[derive(Clone)]
pub struct MultiProgress {
    progress_bars: Arc<Mutex<Vec<Arc<ProgressBar>>>>,
    /// # start_row
    /// the starting row position of the multibar
    /// used when redrawing all the bars, positioning, etc.
    /// Must be updated whenever we have to scroll the terminal
    start_row: Arc<Mutex<u16>>,
    shared_state: &'static Mutex<SharedState>,
}

/// # AddLocation
/// The `AddLocation` enum is used to specify where to add a new progress bar in the `MultiProgress`.
/// Currently only `Bottom` is supported.
#[allow(dead_code)]
#[derive(PartialEq)]
pub enum AddLocation {
    //Top,   -- not working quite right yet
    Bottom,
}

#[allow(dead_code)]
impl MultiProgress {
    /// # new
    /// Creates a new `MultiProgress` instance.
    /// ## Returns
    /// A new `MultiProgress` instance
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// ```
    pub fn new() -> Self {
        MultiProgress {
            progress_bars: Arc::new(Mutex::new(Vec::new())),
            start_row: Arc::new(Mutex::new(if stdout().is_terminal() {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
            shared_state: SharedState::instance(),
        }
    }

    /// # add
    /// Adds a new progress bar to the `MultiProgress` at the bottom.
    /// ## Parameters
    /// - `progress_bar`: The progress bar to add
    /// ## Returns
    /// The `ProgressBar` instance added to the `MultiProgress`
    /// ## Example
    /// ```rust
    /// use progressbar::{MultiProgress, ProgressBar};
    /// let multi_progress = MultiProgress::new();
    /// let progress_bar = ProgressBar::new(100);
    /// multi_progress.add(progress_bar);
    /// ```
    pub fn add(&self, progress_bar: ProgressBar) -> Arc<ProgressBar> {
        self.add_with_location(progress_bar, AddLocation::Bottom)
    }

    /// # add_with_location
    /// Adds a new progress bar to the `MultiProgress` at the specified location.
    /// ## Parameters
    /// - `progress_bar`: The progress bar to add
    /// - `location`: The location to add the progress bar
    /// ## Returns
    /// The `ProgressBar` instance added to the `MultiProgress`
    /// ## Example
    /// ```rust
    /// use progressbar::{MultiProgress, ProgressBar, AddLocation};
    /// let multi_progress = MultiProgress::new();
    /// let progress_bar = ProgressBar::new(100);
    /// multi_progress.add_with_location(progress_bar, AddLocation::Bottom);
    /// ```
    pub fn add_with_location(
        &self,
        progress_bar: ProgressBar,
        _location: AddLocation,
    ) -> Arc<ProgressBar> {
        let mut stdout = stdout();
        let is_terminal = stdout.is_terminal();
        let current_row = self.start_row.lock().unwrap();
        let mut local_current_row = *current_row;
        let arc_progress_bar = Arc::new(progress_bar);
        let mut progress_bars = self.progress_bars.lock().unwrap();
        local_current_row += progress_bars.len() as u16;

        if is_terminal {
            execute!(stdout, MoveTo(0, local_current_row)).unwrap();
        }

        progress_bars.push(arc_progress_bar.clone());

        drop(progress_bars);

        if is_terminal {
            execute!(
                stdout,
                MoveTo(0, local_current_row),
                Clear(ClearType::FromCursorDown)
            )
            .unwrap();
        }
        arc_progress_bar
    }

    /// # remove
    /// Removes the specified progress bar from the `MultiProgress`.
    /// ## Parameters
    /// - `progress_bar`: The progress bar to remove
    /// ## Example
    /// ```rust
    /// use progressbar::{MultiProgress, ProgressBar};
    /// let multi_progress = MultiProgress::new();
    /// let progress_bar = ProgressBar::new(100);
    /// multi_progress.add(progress_bar.clone());
    /// multi_progress.remove(&progress_bar);
    /// ```
    pub fn remove(&self, progress_bar: &ProgressBar) {
        let mut progress_bars = self.progress_bars.lock().unwrap();
        let mut stdout = stdout();
        let is_terminal = stdout.is_terminal();
        if let Some(pos) = progress_bars
            .iter()
            .position(|x| Arc::ptr_eq(&x.progress, &progress_bar.progress))
        {
            let current_row = self.start_row.lock().unwrap();
            progress_bars.remove(pos);
            if is_terminal {
                execute!(
                    stdout,
                    MoveTo(0, *current_row),
                    Clear(ClearType::FromCursorDown)
                )
                .unwrap();
                stdout.flush().unwrap();
            }
            drop(current_row);
            drop(progress_bars);
            //self.draw_all();
        }
    }

    /// # stop_all_spinners
    /// Stops all spinners in the `MultiProgress`.
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// multi_progress.stop_all_spinners();
    /// ```
    fn stop_all_spinners(&self) {
        let progress_bars = self.progress_bars.lock().unwrap();
        for progress_bar in progress_bars.iter() {
            progress_bar.stop_spinner();
        }
    }

    /// # start_all_spinners
    /// Starts all spinners in the `MultiProgress`.
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// multi_progress.start_all_spinners();
    /// ```
    fn start_all_spinners(&self) {
        let progress_bars = self.progress_bars.lock().unwrap();
        for progress_bar in progress_bars.iter() {
            progress_bar.start_spinner();
        }
    }

    fn move_down(&self, value: u16) {
        // move the cursor down and set start row
        let mut stdout = stdout();
        execute!(stdout, MoveToNextLine(value)).unwrap();
        let mut start_row = self.start_row.lock().unwrap();
        let current_row = crossterm::cursor::position().unwrap().1;
        let progress_bar = self.progress_bars.lock().unwrap();
        *start_row = current_row;

        // determine if we are at the bottom and will need to scroll the terminal up
        let (_, rows) = crossterm::terminal::size().unwrap();
        let current_row = crossterm::cursor::position().unwrap().1;

        if current_row > (rows - progress_bar.len() as u16) {
            execute!(stdout, ScrollUp(value), crossterm::cursor::MoveToPreviousLine(value)).unwrap();
            // todo
            // instead of scrollup which causes flickering and other not nice cosmetics, let's try copyring the lines above
            // into an array, then redrawing everything but the first line.  this may eliminate the flickering issue
            
            *start_row = crossterm::cursor::position().unwrap().1;
        }
    }

    /// # move_cursor_to_top
    /// moves the cursor to the top row where the multi will render
    /// # NOTE
    /// This requires a lock on start_row
    fn move_cursor_to_top(&self) {
        let mut stdout = stdout();
        let is_terminal: bool = stdout.is_terminal();
        let mut start_row = self.start_row.lock().unwrap();
        let progress_bars = self.progress_bars.lock().unwrap();

        if is_terminal {
            let (_, rows) = crossterm::terminal::size().unwrap();
            // if the current_row is neart the bottom of the terminal such that we can't
            // redraw all the progress bars, then reposition so that we can
            execute!(stdout, MoveTo(0, *start_row)).unwrap();
            let mut current_row: u16 = crossterm::cursor::position().unwrap().1;
            while current_row > rows - progress_bars.len() as u16 {
                execute!(stdout, MoveUp(1)).unwrap();
                current_row -= 1;
                *start_row = current_row;
            }
        }
    }

    fn render_all(&self, lock: bool) {
        //self.stop_all_spinners();

        let mut stdout = stdout();

        let is_terminal: bool = stdout.is_terminal();
        //let current_row = self.start_row.lock().unwrap();

        if is_terminal {
            let _guard: Option<std::sync::MutexGuard<'_, SharedState>> = if lock {
                Some(self.shared_state.lock().unwrap())
            } else {
                None
            };
            self.move_cursor_to_top();
            execute!(stdout, BeginSynchronizedUpdate).unwrap();
            // execute!(
            //     stdout,
            //     Clear(ClearType::FromCursorDown)
            // )
            // .unwrap();

            let progress_bars = self.progress_bars.lock().unwrap();
            for progress_bar in progress_bars.iter() {
                let current_row = crossterm::cursor::position().unwrap().1;
                if progress_bar.get_style() == ProgressBarStyle::Bar {
                    progress_bar.render_bar();
                } else {
                    progress_bar.render_spinner(false, Some(current_row));
                }
                //self.move_down(1);
                execute!(stdout, MoveToNextLine(1)).unwrap();
            }
            execute!(stdout, EndSynchronizedUpdate).unwrap();
            drop(progress_bars);
            if lock {
                drop(_guard.unwrap());
            }
        }

        //drop(current_row);
        //self.move_cursor_to_top();
        //self.start_all_spinners();
        //stdout.flush().unwrap();
    }

    /// # draw_all
    /// Draws all progress bars in the `MultiProgress`.
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// multi_progress.draw_all();
    /// ```
    pub fn draw_all(&self) {
        let mut stdout = stdout();
        self.render_all(true);
        stdout.flush().unwrap();
    }

    /// # finish_all
    /// Finishes all progress bars in the `MultiProgress`.
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// multi_progress.finish_all();
    /// ```
    pub fn finish_all(&self) {
        self.move_cursor_to_top();
        let progress_bars = self.progress_bars.lock().unwrap();
        for progress_bar in progress_bars.iter() {
            progress_bar.finish();
        }
        let mut stdout = stdout();
        execute!(stdout, MoveDown(progress_bars.len() as u16)).unwrap();
    }

    /// # println
    /// Prints a message to the terminal, above the progress bars. If the terminal is not a TTY,
    /// the message is printed to stdout.
    /// ## Parameters
    /// - `message`: The message to print
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// multi_progress.println("Loading...");
    /// ```
    pub fn println(&self, message: &str) {
        let mut stdout = stdout();
        let is_terminal: bool = stdout.is_terminal();
        if is_terminal {
            // get a lock the shared state instance
            let mut _guard = self.shared_state.lock().expect("Failed to lock shared state");
            self.move_cursor_to_top();
            execute!(
                stdout,
                MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                Print(format!("{}", message)),
            )
            .unwrap();
            self.move_down(1);
            self.render_all(false);
            stdout.flush().unwrap();
            //execute!(stdout, EndSynchronizedUpdate).unwrap();
        } else {
            println!("{}", message);
        }
    }

    /// # eprintln
    /// Prints an error message to the terminal, above the progress bars. If the terminal is not a TTY,
    /// the message is printed to stderr.
    /// ## Parameters
    /// - `message`: The message to print
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// multi_progress.eprintln("Error: Something went wrong");
    /// ```
    pub fn eprintln(&self, message: &str) {

        let mut stdout = stdout();
        let is_terminal: bool = stdout.is_terminal();
        if is_terminal {
            // get a lock the shared state instance
            let mut _guard = self.shared_state.lock().unwrap();
            self.move_cursor_to_top();
            execute!(
                stdout,
                MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                SetForegroundColor(crossterm::style::Color::Red),
                Print(format!("{}", message)),
                ResetColor,
            )
            .unwrap();
            self.move_down(1);
            self.render_all(false);
            stdout.flush().unwrap();
            execute!(stdout, EndSynchronizedUpdate).unwrap();
        } else {
            println!("{}", message);
        }
    }

    /// # set_message
    /// Sets the message to display with the specified progress bar.
    /// ## Parameters
    /// - `progress_bar`: The progress bar to set the message for
    /// - `msg`: The message to display
    /// ## Example
    /// ```rust
    /// use progressbar::{MultiProgress, ProgressBar};
    /// let multi_progress = MultiProgress::new();
    /// let progress_bar = ProgressBar::new(100);
    /// multi_progress.add(progress_bar.clone());
    /// multi_progress.set_message(&progress_bar, "Loading...");
    /// ```
    pub fn set_message(&self, progress_bar: &ProgressBar, msg: &str) {
        let progress_bars = self.progress_bars.lock().unwrap();
        if let Some(pos) = progress_bars
            .iter()
            .position(|x| Arc::ptr_eq(&x.progress, &progress_bar.progress))
        {
            progress_bars[pos].set_message(msg);
            drop(progress_bars);
            self.draw_all();
        }
    }

    /// # increment
    /// Increments the progress of the specified progress bar by the specified value.
    /// ## Parameters
    /// - `progress_bar`: The progress bar to increment
    /// - `value`: The value to increment the progress by
    /// ## Example
    /// ```rust
    /// use progressbar::{MultiProgress, ProgressBar};
    /// let multi_progress = MultiProgress::new();
    /// let progress_bar = ProgressBar::new(100);
    /// multi_progress.add(progress_bar.clone());
    /// multi_progress.increment(&progress_bar, 10);
    /// ```
    pub fn increment(&self, progress_bar: &ProgressBar, value: u32) {
        // get a lock the shared state instance
        let mut _guard = self.shared_state.lock().unwrap();
        // need to move cursor to correct spot to draw the bar
        self.move_cursor_to_top();
        let progress_bars = self.progress_bars.lock().unwrap();
        // iterate the progress bars and move cursor down until we find out progress bar
        for bar in progress_bars.iter() {
            if Arc::ptr_eq(&bar.progress, &progress_bar.progress) {
                bar.increment(value);
                break;
            } else {
                let mut stdout = stdout();
                let is_terminal: bool = stdout.is_terminal();
                if is_terminal {
                    execute!(stdout, MoveDown(1)).unwrap();
                }
            }
        }
        drop(progress_bars);
        self.move_cursor_to_top();
    }

    pub fn set_position(&self, progress_bar: &ProgressBar, value: u32) {
        let mut stdout = stdout();
        let is_terminal: bool = stdout.is_terminal();
        // get a lock the shared state instance
        let mut _guard = self.shared_state.lock().unwrap();
        // need to move cursor to correct spot to draw the bar
        self.move_cursor_to_top();
        let progress_bars = self.progress_bars.lock().unwrap();
        // iterate the progress bars and move cursor down until we find out progress bar
        for bar in progress_bars.iter() {
            if Arc::ptr_eq(&bar.progress, &progress_bar.progress) {
                bar.set_position(value);
                break;
            } else {
                if is_terminal {
                    execute!(stdout, MoveDown(1)).unwrap();
                }
            }
        }
        drop(progress_bars);
        self.move_cursor_to_top();
        stdout.flush().unwrap();
    }

    /// # get_progress_bars_count
    /// Gets the number of progress bars in the `MultiProgress`.
    /// ## Returns
    /// The number of progress bars
    /// ## Example
    /// ```rust
    /// use progressbar::MultiProgress;
    /// let multi_progress = MultiProgress::new();
    /// let count = multi_progress.get_progress_bars_count();
    /// ```
    pub fn get_progress_bars_count(&self) -> usize {
        let progress_bars = self.progress_bars.lock().unwrap();
        let count = progress_bars.len();
        drop(progress_bars);
        count
    }
}

/// # Tests
///
/// Unit tests for the various functions and features of the program.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_new() {
        let progress_bar = ProgressBar::new(100);
        assert_eq!(progress_bar.get_style(), ProgressBarStyle::Bar);
        assert_eq!(progress_bar.get_position(), 0);
    }

    #[test]
    fn test_progress_bar_new_spinner() {
        let progress_bar = ProgressBar::new_spinner();
        assert_eq!(progress_bar.get_style(), ProgressBarStyle::Spinner);
        assert_eq!(progress_bar.get_position(), 0);
        assert_eq!(*progress_bar.is_spinning.lock().unwrap(), false);
    }

    #[test]
    fn test_progress_bar_hidden() {
        let progress_bar = ProgressBar::hidden();
        assert_eq!(progress_bar.get_style(), ProgressBarStyle::Hidden);
        assert_eq!(progress_bar.get_position(), 0);
    }

    #[test]
    fn test_progress_bar_with_message_chain() {
        let progress_bar = ProgressBar::new(100).with_message("Loading...");
        assert_eq!(progress_bar.get_style(), ProgressBarStyle::Bar);
        assert_eq!(progress_bar.get_position(), 0);
        assert_eq!(progress_bar.get_message(), "Loading...");
    }

    #[test]
    fn test_progress_bar_increment() {
        let progress_bar = ProgressBar::new(100);
        progress_bar.increment(10);
        assert_eq!(progress_bar.get_position(), 10);
    }

    #[test]
    fn test_progress_bar_increment_more_than_total() {
        let progress_bar = ProgressBar::new(100);
        progress_bar.increment(200);
        assert_eq!(progress_bar.get_position(), 100);
    }

    #[test]
    fn test_progress_bar_set_message() {
        let progress_bar = ProgressBar::new(100);
        progress_bar.set_message("Loading...");
        assert_eq!(progress_bar.get_message(), "Loading...");
    }

    // #[test]
    // fn test_progress_bar_set_row() {
    //     let progress_bar = ProgressBar::new(100);
    //     progress_bar.set_row(5);
    //     assert_eq!(*progress_bar.start_row.lock().unwrap(), 5);
    // }

    #[test]
    fn test_progress_bar_println() {
        let progress_bar = ProgressBar::new(100);
        progress_bar.println("Loading...");
    }

    #[test]
    fn test_progress_bar_with_start_spinner() {
        let progress_bar = ProgressBar::new_spinner().with_start_spinner();
        assert_eq!(progress_bar.get_style(), ProgressBarStyle::Spinner);
        assert_eq!(*progress_bar.is_spinning.lock().unwrap(), true);
    }

    #[test]
    fn test_progress_bar_start_spinner() {
        let progress_bar = ProgressBar::new_spinner();
        progress_bar.start_spinner();
        assert_eq!(progress_bar.get_style(), ProgressBarStyle::Spinner);
        assert_eq!(*progress_bar.is_spinning.lock().unwrap(), true);
    }

    #[test]
    fn test_progress_bar_stop_spinner() {
        let progress_bar = ProgressBar::new_spinner().with_start_spinner();
        progress_bar.stop_spinner();
        assert_eq!(*progress_bar.is_spinning.lock().unwrap(), false);
    }

    #[test]
    fn test_progress_bar_draw_spinner() {
        let progress_bar = ProgressBar::new_spinner();
        progress_bar.render_spinner(false, None);
    }

    #[test]
    fn test_progress_bar_draw() {
        let progress_bar = ProgressBar::new(100);
        progress_bar.draw();
    }

    #[test]
    fn test_progress_bar_finish() {
        let progress_bar = ProgressBar::new_spinner().with_start_spinner();
        progress_bar.finish();
        assert_eq!(*progress_bar.is_spinner_thread_running.lock().unwrap(), false);
    }

    #[test]
    fn test_multi_progress_new() {
        let multi_progress = MultiProgress::new();
        assert_eq!(multi_progress.get_progress_bars_count(), 0);
    }

    #[test]
    fn test_multi_progress_add() {
        let multi_progress = MultiProgress::new();
        let progress_bar = ProgressBar::new(100);
        multi_progress.add(progress_bar);
        assert_eq!(multi_progress.get_progress_bars_count(), 1);
    }

    #[test]
    fn test_multi_progress_add_with_location() {
        let multi_progress = MultiProgress::new();
        let progress_bar = ProgressBar::new(100);
        multi_progress.add_with_location(progress_bar, AddLocation::Bottom);
        assert_eq!(multi_progress.get_progress_bars_count(), 1);
    }

    #[test]
    fn test_multi_progress_remove() {
        let multi_progress = MultiProgress::new();
        let progress_bar = ProgressBar::new(100);
        multi_progress.add(progress_bar.clone());
        multi_progress.remove(&progress_bar);
        assert_eq!(multi_progress.get_progress_bars_count(), 0);
    }

    #[test]
    fn test_multi_progress_stop_all_spinners() {
        let multi_progress = MultiProgress::new();
        let spinner = multi_progress.add(ProgressBar::new_spinner().with_start_spinner());
        multi_progress.stop_all_spinners();
        assert_eq!(*spinner.is_spinning.lock().unwrap(), false);
    }

    #[test]
    fn test_multi_progress_start_all_spinners() {
        let multi_progress = MultiProgress::new();
        let spinner = multi_progress.add(ProgressBar::new_spinner());
        multi_progress.start_all_spinners();
        assert_eq!(*spinner.is_spinning.lock().unwrap(), true);
    }

    #[test]
    fn test_multi_progress_draw_all() {
        let multi_progress = MultiProgress::new();
        let progress_bar = ProgressBar::new(100);
        multi_progress.add(progress_bar);
        multi_progress.draw_all();
    }

    #[test]
    fn test_multi_progress_finish_all() {
        let multi_progress = MultiProgress::new();
        let progress_bar = multi_progress.add(ProgressBar::new_spinner().with_start_spinner());
        multi_progress.finish_all();
        assert_eq!(*progress_bar.is_spinner_thread_running.lock().unwrap(), false);
    }

    #[test]
    fn test_multi_progress_println() {
        let multi_progress = MultiProgress::new();
        multi_progress.println("Loading...");
    }

    #[test]
    fn test_multi_progress_eprintln() {
        let multi_progress = MultiProgress::new();
        multi_progress.eprintln("Error: Something went wrong");
    }

    #[test]
    fn test_multi_progress_set_message() {
        let multi_progress = MultiProgress::new();
        let progress_bar = multi_progress.add(ProgressBar::new(100));
        multi_progress.set_message(&progress_bar, "Loading...");
        assert_eq!(progress_bar.get_message(), "Loading...");
    }

    #[test]
    fn test_multi_progress_increment() {
        let multi_progress = MultiProgress::new();
        let progress_bar = multi_progress.add(ProgressBar::new(100));
        multi_progress.increment(&progress_bar, 10);
        assert_eq!(progress_bar.get_position(), 10);
    }

    #[test]
    fn test_multi_progress_get_progress_bars_count() {
        let multi_progress = MultiProgress::new();
        let _progress_bar = multi_progress.add(ProgressBar::new(100));
        assert_eq!(multi_progress.get_progress_bars_count(), 1);
    }
}
