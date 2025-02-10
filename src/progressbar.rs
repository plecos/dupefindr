//! This module provides functionality for creating and managing progress bars and spinners
//! in the terminal. It includes the `ProgressBar` struct for individual progress bars and
//! spinners, and the `MultiProgress` struct for managing multiple progress bars.
//! It was inspired by the `indicatif` crate, but is much simpler and more lightweight.  It also
//! is written specifically for use with the `crossterm` crate.

use atty::Stream;
use crossterm::cursor::{MoveDown, MoveToNextLine, RestorePosition, SavePosition};
use crossterm::queue;
use crossterm::style::{Print, SetForegroundColor};
use crossterm::terminal::ScrollUp;
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use std::io::{stdout, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// The spinner characters to use for the spinner progress bar
const SPINNER_CHARS: &[char] = &[
    '⠁', '⠁', '⠉', '⠙', '⠚', '⠒', '⠂', '⠂', '⠒', '⠲', '⠴', '⠤', '⠄', '⠄', '⠤', '⠠', '⠠', '⠤', '⠦',
    '⠖', '⠒', '⠐', '⠐', '⠒', '⠓', '⠋', '⠉', '⠈', '⠈',
];

/// # ProgressBarStyle
/// The `ProgressBarStyle` enum is used to specify the style of the progress bar.
#[derive(Clone, Copy, PartialEq)]
pub enum ProgressBarStyle {
    /// A standard progress bar
    Bar,
    /// A spinner progress bar
    Spinner,
    /// A hidden progress bar
    Hidden,
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
    /// # start_row
    /// The row where the progress bar starts
    start_row: Arc<Mutex<u16>>,
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
            start_row: Arc::new(Mutex::new(if atty::is(atty::Stream::Stdout) {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
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
            start_row: Arc::new(Mutex::new(if atty::is(atty::Stream::Stdout) {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
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
            start_row: Arc::new(Mutex::new(if atty::is(atty::Stream::Stdout) {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
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
        let mut progress = self.progress.lock().unwrap();
        *progress += value;
        if *progress > self.total {
            *progress = self.total;
        }
    }

    /// # get_progress
    /// Gets the current progress of the progress bar.
    /// ## Returns
    /// The current progress value
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// let progress = progress_bar.get_progress();
    /// assert_eq!(progress, 0);
    /// ```
    pub fn get_progress(&self) -> u32 {
        let progress = self.progress.lock().unwrap();
        *progress
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
    }

    /// # set_row
    /// Sets the row where the progress bar starts.
    /// ## Parameters
    /// - `row`: The row where the progress bar starts
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new(100);
    /// progress_bar.set_row(5);
    /// ```
    pub fn set_row(&self, row: u16) {
        let mut start_row = self.start_row.lock().unwrap();
        *start_row = row;
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
        // If the terminal is a TTY, print the message above the progress bar
        if atty::is(Stream::Stdout) {
            let mut stdout = stdout();
            execute!(stdout, SavePosition, Clear(ClearType::CurrentLine)).unwrap();
            writeln!(stdout, "{}", message).unwrap();
            execute!(stdout, RestorePosition, MoveDown(1)).unwrap();
            stdout.flush().unwrap();
            self.draw();
        } else {
            // If the terminal is not a TTY, print the message to stdout
            // this is included for testing purposes where there is no TTY or for redirection to a file
            println!("{}", message);
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
        let s = self.clone();
        *is_spinning.lock().unwrap() = true;
        let current_row = Arc::clone(&self.start_row);

        // Start a new thread to draw the spinner
        // This allows the spinner to run independently of the main thread
        // and update the spinner while the main thread is doing other work
        // This is useful for long running tasks where the spinner needs to be updated
        // while the main thread is busy
        thread::spawn(move || {
            while *is_spinning.lock().unwrap() {
                s.draw_spinner(true);
                // Check every 100ms to see if the spinner should stop
                // this allows the spinner to stop quickly when the main thread
                // sets is_spinning to false, and provides 100ms for animation of the spinner
                for _ in 0..10 {
                    if !*is_spinning.lock().unwrap() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
            // Clear the spinner when it stops (only on TTY)
            if atty::is(Stream::Stdout) {
                let mut stdout = stdout();
                queue!(
                    stdout,
                    MoveTo(0, *current_row.lock().unwrap()),
                    Clear(ClearType::CurrentLine)
                )
                .unwrap();
                stdout.flush().unwrap();
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

    /// # draw_spinner
    /// Draws the spinner for the progress bar.
    /// ## Parameters
    /// - `inc_index`: A boolean value indicating whether to increment the spinner index
    /// ## Example
    /// ```rust
    /// use progressbar::ProgressBar;
    /// let progress_bar = ProgressBar::new_spinner();
    /// progress_bar.draw_spinner(false);
    /// ```
    pub fn draw_spinner(&self, inc_index: bool) {
        // ignore if not a spinner style
        if !self.style.eq(&ProgressBarStyle::Spinner) {
            return;
        }
        // only on TTY
        if atty::is(Stream::Stdout) {
            let mut stdout = stdout();
            let message = Arc::clone(&self.message);
            let mut index = self.spinner_index.lock().unwrap();
            let current_row = self.start_row.lock().unwrap();

            queue!(
                stdout,
                SavePosition,
                MoveTo(0, *current_row),
                Clear(ClearType::CurrentLine),
                Print(format!(
                    "{} {}",
                    SPINNER_CHARS[*index],
                    *message.lock().unwrap()
                )),
            )
            .unwrap();
            queue!(stdout, RestorePosition).unwrap();
            stdout.flush().unwrap();
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
        if !atty::is(Stream::Stdout) {
            return;
        }
        let mut stdout = stdout();

        if self.style.eq(&ProgressBarStyle::Hidden) {
            return;
        } else if self.style.eq(&ProgressBarStyle::Spinner) {
            self.draw_spinner(false);
            return;
        } else {
            let progress = self.get_progress();
            let percentage = (progress as f64 / self.total as f64) * 100.0;
            let message = self.message.lock().unwrap();
            let current_row = self.start_row.lock().unwrap();

            queue!(
                stdout,
                SavePosition,
                MoveTo(0, *current_row),
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
            queue!(stdout, RestorePosition).unwrap();
            stdout.flush().unwrap();
        }
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
            let mut is_running = self.is_spinning.lock().unwrap();
            *is_running = false;
        }
    }
}

/// # MultiProgress
/// The `MultiProgress` struct is used to manage multiple progress bars and spinners.
/// It provides functionality for adding, removing, and updating progress bars, as well as
/// printing messages to the terminal.
#[derive(Clone)]
pub struct MultiProgress {
    progress_bars: Arc<Mutex<Vec<Arc<ProgressBar>>>>,
    start_row: Arc<Mutex<u16>>,
}

/// # AddLocation
/// The `AddLocation` enum is used to specify where to add a new progress bar in the `MultiProgress`.
/// Currently only `Bottom` is supported.
#[derive(PartialEq)]
pub enum AddLocation {
    //Top,   -- not working quite right yet
    Bottom,
}

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
            start_row: Arc::new(Mutex::new(if atty::is(atty::Stream::Stdout) {
                crossterm::cursor::position().unwrap().1.into()
            } else {
                0
            })),
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
        let current_row = self.start_row.lock().unwrap();
        let mut local_current_row = *current_row;
        let arc_progress_bar = Arc::new(progress_bar);
        let mut progress_bars = self.progress_bars.lock().unwrap();
        local_current_row += progress_bars.len() as u16;

        if atty::is(Stream::Stdout) {
            execute!(stdout, MoveTo(0, local_current_row)).unwrap();
        }

        progress_bars.push(arc_progress_bar.clone());

        drop(progress_bars);

        if atty::is(Stream::Stdout) {
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
        if let Some(pos) = progress_bars
            .iter()
            .position(|x| Arc::ptr_eq(&x.progress, &progress_bar.progress))
        {
            let current_row = self.start_row.lock().unwrap();
            progress_bars.remove(pos);
            if atty::is(Stream::Stdout) {
                let mut stdout = stdout();
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
        // let progress_bars = self.progress_bars.lock().unwrap();
        // for progress_bar in progress_bars.iter() {
        //     progress_bar.stop_spinner();
        // }
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
        // let progress_bars = self.progress_bars.lock().unwrap();
        // for progress_bar in progress_bars.iter() {
        //     progress_bar.start_spinner();
        // }
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
        self.stop_all_spinners();
        let progress_bars = self.progress_bars.lock().unwrap();
        let current_row = self.start_row.lock().unwrap();
        let mut bar_row = *current_row;

        if atty::is(Stream::Stdout) {
            let mut stdout = stdout();

            queue!(
                stdout,
                SavePosition,
                MoveTo(0, *current_row),
                Clear(ClearType::CurrentLine)
            )
            .unwrap();

            for progress_bar in progress_bars.iter() {
                progress_bar.set_row(bar_row);
                progress_bar.draw();
                queue!(stdout, MoveToNextLine(1)).unwrap();
                bar_row += 1;
            }
            drop(progress_bars);
            queue!(stdout, RestorePosition).unwrap();
            stdout.flush().unwrap();
        }
        self.start_all_spinners();
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
        let progress_bars = self.progress_bars.lock().unwrap();
        for progress_bar in progress_bars.iter() {
            progress_bar.finish();
        }
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
        if atty::is(Stream::Stdout) {
            self.stop_all_spinners();
            let mut stdout = stdout();
            let mut current_row = self.start_row.lock().unwrap();
            let progress_bars = self.progress_bars.lock().unwrap();
            queue!(
                stdout,
                MoveTo(0, *current_row),
                Clear(ClearType::CurrentLine),
                Print(message)
            )
            .unwrap();

            // get number of rows on terminal
            let (_, rows) = crossterm::terminal::size().unwrap();

            if rows - 2 - (progress_bars.len() as u16) < *current_row {
                queue!(stdout, ScrollUp(1)).unwrap();
            } else {
                *current_row += 1;
            }
            drop(progress_bars);
            drop(current_row);
            self.draw_all();
            stdout.flush().unwrap();
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
        if atty::is(Stream::Stdout) {
            self.stop_all_spinners();
            let mut stdout = stdout();
            let mut current_row = self.start_row.lock().unwrap();
            let progress_bars = self.progress_bars.lock().unwrap();
            queue!(
                stdout,
                SetForegroundColor(crossterm::style::Color::Red),
                MoveTo(0, *current_row),
                Clear(ClearType::CurrentLine),
                Print(message),
                crossterm::style::ResetColor,
            )
            .unwrap();
            queue!(stdout, MoveToNextLine(1)).unwrap();
            *current_row += 1;
            drop(progress_bars);
            drop(current_row);
            self.draw_all();
            stdout.flush().unwrap();
        } else {
            eprintln!("{}", message);
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
        let progress_bars = self.progress_bars.lock().unwrap();
        if let Some(pos) = progress_bars
            .iter()
            .position(|x| Arc::ptr_eq(&x.progress, &progress_bar.progress))
        {
            let bar = progress_bars[pos].clone();
            progress_bars[pos].increment(value);
            drop(progress_bars);
            bar.draw();
        }
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
