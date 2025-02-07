use crossterm::cursor::{
    MoveDown, MoveLeft, MoveToNextLine, RestorePosition, SavePosition,
};
use crossterm::queue;
use crossterm::style::Print;
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

const SPINNER_CHARS: &[char] = &[
    '⠁', '⠁', '⠉', '⠙', '⠚', '⠒', '⠂', '⠂', '⠒', '⠲', '⠴', '⠤', '⠄', '⠄', '⠤', '⠠', '⠠', '⠤', '⠦',
    '⠖', '⠒', '⠐', '⠐', '⠒', '⠓', '⠋', '⠉', '⠈', '⠈',
];

#[derive(Clone)]
pub struct ProgressBar {
    progress: Arc<Mutex<u32>>,
    total: u32,
    is_spinner: bool,
    is_spinning: Arc<Mutex<bool>>,
    message: Arc<Mutex<String>>,
    hidden: Arc<Mutex<bool>>,
    spinner_index: Arc<Mutex<usize>>,
    start_row: Arc<Mutex<u16>>,
}

#[allow(dead_code)]
impl ProgressBar {
    pub fn new(total: u32) -> Self {
        ProgressBar {
            progress: Arc::new(Mutex::new(0)),
            total,
            is_spinner: false,
            is_spinning: Arc::new(Mutex::new(false)),
            message: Arc::new(Mutex::new(String::new())),
            hidden: Arc::new(Mutex::new(false)),
            spinner_index: Arc::new(Mutex::new(0)),
            start_row: Arc::new(crossterm::cursor::position().unwrap().1.into()),
        }
    }

    pub fn new_spinner() -> Self {
        let progress_bar = ProgressBar {
            progress: Arc::new(Mutex::new(0)),
            total: 1, // Spinner doesn't need a total value
            is_spinner: true,
            is_spinning: Arc::new(Mutex::new(false)),
            message: Arc::new(Mutex::new(String::new())),
            hidden: Arc::new(Mutex::new(false)),
            spinner_index: Arc::new(Mutex::new(0)),
            start_row: Arc::new(crossterm::cursor::position().unwrap().1.into()),
        };
        progress_bar
    }

    pub fn hidden() -> Self {
        ProgressBar {
            progress: Arc::new(Mutex::new(0)),
            total: 1,
            is_spinner: false,
            is_spinning: Arc::new(Mutex::new(false)),
            message: Arc::new(Mutex::new(String::new())),
            hidden: Arc::new(Mutex::new(true)),
            spinner_index: Arc::new(Mutex::new(0)),
            start_row: Arc::new(crossterm::cursor::position().unwrap().1.into()),
        }
    }

    pub fn with_message(self, msg: &str) -> Self {
        self.set_message(msg);
        self
    }

    pub fn increment(&self, value: u32) {
        let mut progress = self.progress.lock().unwrap();
        *progress += value;
        if *progress > self.total {
            *progress = self.total;
        }
        //drop(progress); // Release the lock before drawing
        //self.draw();
    }

    pub fn get_progress(&self) -> u32 {
        let progress = self.progress.lock().unwrap();
        *progress
    }

    pub fn set_message(&self, msg: &str) {
        let mut message = self.message.lock().unwrap();
        *message = msg.to_string();
    }

    pub fn set_row(&self, row: u16) {
        let mut start_row = self.start_row.lock().unwrap();
        *start_row = row;
    }

    pub fn println(&self, message: &str) {
        let mut stdout = stdout();
        execute!(stdout, SavePosition, Clear(ClearType::CurrentLine)).unwrap();
        writeln!(stdout, "{}", message).unwrap();
        execute!(stdout, RestorePosition, MoveDown(1)).unwrap();
        stdout.flush().unwrap();
        self.draw();
    }

    pub fn with_start_spinner(self) -> Self {
        self.start_spinner();
        self
    }

    pub fn start_spinner(&self) {
        if !self.is_spinner {
            return;
        }

        let progress = Arc::clone(&self.progress);
        let total = self.total;
        let is_spinning = Arc::clone(&self.is_spinning);
        let s = self.clone();
        *is_spinning.lock().unwrap() = true;
        let current_row = Arc::clone(&self.start_row);

        thread::spawn(move || {
            while *is_spinning.lock().unwrap() {
                if *progress.lock().unwrap() >= total {
                    break;
                }
                s.draw_spinner(true);
                for _ in 0..10 {
                    if !*is_spinning.lock().unwrap() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
            let mut stdout = stdout();
            queue!(stdout,MoveTo(0, *current_row.lock().unwrap()), Clear(ClearType::CurrentLine)).unwrap();
            stdout.flush().unwrap();
        });
    }

    pub fn stop_spinner(&self) {
        let mut is_spinning = self.is_spinning.lock().unwrap();
        *is_spinning = false;
    }

    pub fn draw_spinner(&self, inc_index: bool) {
        if !self.is_spinner {
            return;
        }
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

    pub fn draw(&self) {
        let mut stdout = stdout();
        let hidden = self.hidden.lock().unwrap();
        if *hidden {
            return;
        } else if self.is_spinner {
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

    pub fn finish(&self) {
        if self.is_spinner {
            let mut is_running = self.is_spinning.lock().unwrap();
            *is_running = false;
        }
    }
}

#[derive(Clone)]
pub struct MultiProgress {
    progress_bars: Arc<Mutex<Vec<Arc<ProgressBar>>>>,
    start_row: Arc<Mutex<u16>>,
}

#[derive(PartialEq)]
pub enum AddLocation {
    //Top,   -- not working quite right yet
    Bottom,
}

impl MultiProgress {
    pub fn new() -> Self {
        MultiProgress {
            progress_bars: Arc::new(Mutex::new(Vec::new())),
            start_row: Arc::new(crossterm::cursor::position().unwrap().1.into()),
        }
    }

    pub fn add(&self, progress_bar: ProgressBar) -> Arc<ProgressBar> {
        self.add_with_location(progress_bar, AddLocation::Bottom)
    }

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
        execute!(stdout, MoveTo(0,local_current_row)).unwrap();

        //if location == AddLocation::Top {
        //    progress_bars.insert(0, arc_progress_bar.clone());
        //} else {
            progress_bars.push(arc_progress_bar.clone());
        //}
        //progress_bars.push(arc_progress_bar.clone());
        // if progress_bars.len() > 1 {
        //     execute!(stdout, MoveDown(1)).unwrap();
        // }
        
        drop(progress_bars);
        //drop(current_row);
        //self.draw_all();
        
        queue!(stdout, MoveTo(0, local_current_row), Clear(ClearType::FromCursorDown)).unwrap();
        arc_progress_bar
    }

    pub fn remove(&self, progress_bar: &ProgressBar) {
        let mut progress_bars = self.progress_bars.lock().unwrap();
        if let Some(pos) = progress_bars
            .iter()
            .position(|x| Arc::ptr_eq(&x.progress, &progress_bar.progress))
        {
            let current_row = self.start_row.lock().unwrap();
            progress_bars.remove(pos);
            let mut stdout = stdout();
            execute!(
                stdout,
                MoveTo(0, *current_row),
                Clear(ClearType::FromCursorDown)
            )
            .unwrap();
            stdout.flush().unwrap();
            drop(current_row);
            drop(progress_bars);
            //self.draw_all();
        }
    }

    fn stop_all_spinners(&self) {
        // let progress_bars = self.progress_bars.lock().unwrap();
        // for progress_bar in progress_bars.iter() {
        //     progress_bar.stop_spinner();
        // }
    }

    fn start_all_spinners(&self) {
        // let progress_bars = self.progress_bars.lock().unwrap();
        // for progress_bar in progress_bars.iter() {
        //     progress_bar.start_spinner();
        // }
    }

    pub fn draw_all(&self) {
        self.stop_all_spinners();
        let progress_bars = self.progress_bars.lock().unwrap();
        let current_row = self.start_row.lock().unwrap();
        let mut bar_row = *current_row;
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
        self.start_all_spinners();
    }

    pub fn finish_all(&self) {
        let progress_bars = self.progress_bars.lock().unwrap();
        for progress_bar in progress_bars.iter() {
            progress_bar.finish();
        }
    }

    pub fn println(&self, message: &str) {
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
    }

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

    pub fn get_progress_bars_count(&self) -> usize {
        let progress_bars = self.progress_bars.lock().unwrap();
        let count = progress_bars.len();
        drop(progress_bars);
        count
    }
}
