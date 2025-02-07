/// A tool to find duplicate files and perform various operations on them such as finding, moving, copying, and deleting.
///
/// # Arguments
///
/// * `path` - The directory to search for duplicates in.
/// * `wildcard` - Wildcard pattern to search for. Example: `*.txt`.
/// * `exclusion_wildcard` - Wildcard pattern to exclude from search. Example: `*.txt`.
/// * `recursive` - Recursively search for duplicates.
/// * `debug` - Display debug information.
/// * `include_empty_files` - Include empty files in the search.
/// * `dry_run` - Dry run the program. This will not delete or modify any files.
/// * `include_hidden_files` - Include hidden files in the search.
/// * `quiet` - Hide progress indicators.
/// * `verbose` - Display verbose output.
///
/// # Commands
///
/// * `find` - Find duplicate files.
/// * `move` - Move duplicate files to a new location.
/// * `copy` - Copy duplicate files to a new location.
/// * `delete` - Delete duplicate files.
///
///

///

///
/// # FileOperations
///
/// Trait for file operations such as copy, move, and delete.
///
/// * `copy` - Copy a file from source to destination.
///
/// # RealFileOperations
///
/// Implementation of `FileOperations` for real file operations.
///
/// # MockFileOperationsOk
///
/// Mock implementation of `FileOperations` that always succeeds.
///
/// # MockFileOperationsError
///
/// Mock implementation of `FileOperations` that always fails.
///
/// # Functions
///

/// * `get_command_line_arguments` - Parse and return command line arguments.
/// * `start_search` - Start the search for duplicate files.
/// * `get_files_in_directory` - Get files in the specified directory.
/// * `identify_duplicates` - Identify duplicate files based on their hash.
/// * `process_duplicates` - Process the identified duplicate files.
/// * `process_a_duplicate_file` - Process a single duplicate file based on the command.
/// * `get_hash_of_file` - Get the MD5 hash of a file.
/// * `get_md5_hash` - Get the MD5 hash of a buffer.
/// * `select_duplicate_files` - Select the file to keep and the duplicates to process based on the selection method.
///
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::style::{Color, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, execute, queue, style};
use glob;
use md5::{self, Digest};
use progressbar::AddLocation;
use std::collections::HashMap;
use std::io::{self, stdout, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::thread::yield_now;
use std::time::Instant;
use std::time::UNIX_EPOCH;
use std::{fs, thread};
use threadpool::ThreadPool;

mod progressbar;

#[derive(Parser, Debug)]
#[command(name = "Dupefindr", version)]
#[command(about = "A tool to find duplicate files", long_about = None)]
#[command(propagate_version = true)]
#[command(author = "Ken Salter")]
struct Args {
    #[command(flatten)]
    shared: SharedOptions,

    #[command(subcommand)]
    command: Commands,
}

/// # SharedOptions
/// Struct representing the shared options.
#[derive(Parser, Debug, Clone)]
struct SharedOptions {
    /// The directory to search for duplicates in.
    #[arg(short, long, default_value = ".")]
    path: String,

    /// wildcard pattern to search for
    /// Example: *.txt
    #[arg(short, long, default_value = "*")]
    wildcard: String,

    /// wildcard pattern to exclude fo
    /// Example: *.txt
    #[arg(long, default_value = "")]
    exclusion_wildcard: String,

    /// Recursively search for duplicates
    #[arg(short, long)]
    recursive: bool,

    /// Display debug information
    #[arg(long, default_value = "false")]
    debug: bool,

    /// Include empty files
    #[arg(long, short = '0', default_value = "false")]
    include_empty_files: bool,

    /// Dry run the program
    /// This will not delete or modify any files
    #[arg(long, default_value = "false")]
    dry_run: bool,

    /// Include hidden files
    #[arg(long, short = 'H', default_value = "false")]
    include_hidden_files: bool,

    /// Hide progress indicators
    #[arg(short, long, default_value = "false")]
    quiet: bool,

    /// Display verbose output
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

/// # Duplicate Selection Method
///
/// * `Newest` - Select the newest file to keep.
/// * `Oldest` - Select the oldest file to keep.
/// * `Interactive` - Prompt user to select file to keep
#[derive(ValueEnum, Debug, Clone, PartialEq)]
enum DuplicateSelectionMethod {
    Newest,
    Oldest,
    Interactive,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Commands {
    #[command(name = "find", about = "Find duplicate files")]
    FindDuplicates {
        /// Method to select the file to keep
        /// Example: newest, oldest, largest, smallest
        #[arg(short, long, default_value = "newest")]
        method: DuplicateSelectionMethod,
    },

    #[command(name = "move", about = "Move duplicate files to a new location")]
    MoveDuplicates {
        /// The directory to move to.
        #[arg(short, long)]
        location: String,

        /// Method to select the file to keep
        /// Example: newest, oldest, largest, smallest
        #[arg(short, long, default_value = "newest")]
        method: DuplicateSelectionMethod,
    },
    #[command(name = "copy", about = "Copy duplicate files to a new location")]
    CopyDuplicates {
        /// The directory to copy to.
        #[arg(short, long)]
        location: String,

        /// Method to select the file to keep
        #[arg(short, long, default_value = "newest")]
        method: DuplicateSelectionMethod,
    },
    #[command(name = "delete", about = "Delete duplicate files")]
    DeleteDuplicates {
        /// Method to select the file to keep
        /// Example: newest, oldest, largest, smallest
        #[arg(short, long, default_value = "newest")]
        method: DuplicateSelectionMethod,
    },
}

/// # FileInfo
///
/// Struct representing file information.
///
/// * `path` - Path to the file.
/// * `size` - Size of the file in bytes.
/// * `created_at` - Creation time of the file.
/// * `modified_at` - Last modified time of the file.
#[derive(Debug, Clone)]
struct FileInfo {
    path: String,
    size: u64,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
}

/// # DuplicateFileSet
///
/// Struct representing a set of duplicate files.
///
/// * `keeper` - The file to keep.
/// * `extras` - The duplicate files.
#[derive(Debug, Clone)]
struct DuplicateFileSet {
    keeper: Option<FileInfo>,
    extras: Vec<FileInfo>,
}

/// # SearchResults
/// Struct representing the search results.
/// * `number_duplicates` - The number of duplicate sets found.
/// * `total_size` - The total size of the duplicates found.
#[derive(Debug, Clone)]
struct SearchResults {
    number_duplicates: usize,
    total_size: usize,
}

/// # FileOperations
/// Trait for file operations such as copy, move, and delete.
/// * `copy` - Copy a file from source to destination.
/// * `remove_file` - Remove a file.
/// * `rename` - Rename a file.
trait FileOperations {
    fn copy(&self, source: &str, destination: &str) -> Result<(), std::io::Error>;
    fn remove_file(&self, source: &str) -> Result<(), std::io::Error>;
    fn rename(&self, source: &str, destination: &str) -> Result<(), std::io::Error>;
}

/// # RealFileOperations
/// Implementation of `FileOperations` for real file operations.
/// * `copy` - Copy a file from source to destination.
/// * `remove_file` - Remove a file.
/// * `rename` - Rename a file.
struct RealFileOperations;

impl FileOperations for RealFileOperations {
    #[cfg(not(tarpaulin_include))]
    fn copy(&self, source: &str, destination: &str) -> Result<(), std::io::Error> {
        match std::fs::copy(source, destination) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    #[cfg(not(tarpaulin_include))]
    fn remove_file(&self, source: &str) -> Result<(), std::io::Error> {
        match std::fs::remove_file(source) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    #[cfg(not(tarpaulin_include))]
    fn rename(&self, source: &str, destination: &str) -> Result<(), std::io::Error> {
        match std::fs::rename(source, destination) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// # TerminalGuard
/// Struct to guard the terminal and reset it when dropped.
/// * `drop` - Reset the terminal.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        println!("TerminalGuard::drop called");
        reset_terminal();
    }
}

/// # myprintln
/// Macro to print a line to the terminal.
/// * `()` - Print a blank line.
/// * `($($arg:tt)*)` - Print the formatted string.
/// # Examples
/// ```
/// myprintln!();
/// myprintln!("Hello, world!");
/// ```
macro_rules! myprintln {
    () => {{
        let _ = execute!(
            stdout(),
            cursor::MoveToNextLine(1),
        );
        io::stdout().flush().unwrap();
    }};
    ($($arg:tt)*) => {{
        let formatted_string = format!($($arg)*);
        let _ = execute!(
            stdout(),
            style::Print(&formatted_string),
            cursor::MoveToNextLine(1),
        );
        io::stdout().flush().unwrap();
    }};
}

/// # myeprintln
/// Macro to print a line to the terminal in red.
/// * `()` - Print a blank line.
/// * `($($arg:tt)*)` - Print the formatted string.
/// # Examples
/// ```
/// myeprintln!();
/// myeprintln!("Error: Something went wrong");
/// ```
macro_rules! myeprintln {
    () => {{
        let _ = execute!(
            stdout(),
            cursor::MoveToNextLine(1),
        );
        io::stdout().flush().unwrap();
    }};
    ($($arg:tt)*) => {{
        let formatted_string = format!($($arg)*);
        let _ = execute!(
            stdout(),
            style::SetForegroundColor(Color::Red),
            style::Print(&formatted_string),
            style::ResetColor,
            cursor::MoveToNextLine(1),
        );
        io::stdout().flush().unwrap();
    }};
}

/// * `main` - Entry point of the program.
#[cfg(not(tarpaulin_include))]
fn main() {
    // Record the start time
    let start = Instant::now();

    let file_ops = RealFileOperations;
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    // Create an instance of TerminalGuard that will be dropped when main exits
    let _guard = TerminalGuard;

    // before enabling raw mode, we need to test if the command line args passed in were valid
    // if they aren't then have print the error and exit
    match Args::try_parse() {
        Ok(_) => {}
        Err(e) => {
            myprintln!("{}", e);
            myprintln!();
            std::process::exit(-1);
        }
    }

    setup_terminal();

    print_banner();

    let args = get_command_line_arguments();

    setup_ctrlc_handler(args.shared.debug, r.clone());

    match start_search(&file_ops, &args, r) {
        Ok(search_results) => {
            let duration = start.elapsed();
            myprintln!("Elapsed time: {}", humantime::format_duration(duration));
            if search_results.number_duplicates == 0 {
                myprintln!("No duplicates found");
            } else {
                myprintln!(
                    "Found {} set of duplicates with total size {}",
                    search_results.number_duplicates,
                    bytesize::ByteSize(search_results.total_size.try_into().unwrap())
                );
            }
            reset_terminal();
            std::process::exit(search_results.number_duplicates.try_into().unwrap());
        }
        Err(e) => {
            myeprintln!("Error: {}", e);
            reset_terminal();
            std::process::exit(-1)
        }
    }
}

/// # print_banner
/// Function to print the banner to the terminal.
fn print_banner() {
    let _ = queue!(
        stdout(),
        SetAttribute(style::Attribute::Bold),
        style::Print("dupefindr"),
        cursor::MoveToNextLine(2),
        SetAttribute(style::Attribute::Reset),
    );
    let _ = stdout().flush();
}

/// # setup_ctrlc_handler
/// Function to setup the ctrl-c handler.
fn setup_ctrlc_handler(debug: bool, running: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    // spawn a thread that will get key events and check for ctrl-c
    std::thread::spawn(move || -> Result<(), anyhow::Error> {
        loop {
            // using a 10 ms timeout to be cpu friendly
            if event::poll(std::time::Duration::from_millis(10))? {
                if let Event::Key(key_event) = event::read()? {
                    if key_event.code == KeyCode::Char('c')
                        && key_event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        // signal by settting our AtomicBool to false
                        running.store(false, std::sync::atomic::Ordering::SeqCst);
                        if debug {
                            myprintln!("CTRL-C detected");
                        }
                    }
                }
            } else {
                yield_now();
            }
        }
    });
}

/// # setup_terminal
/// Setup the terminal for the program.
fn setup_terminal() {
    let _ = terminal::enable_raw_mode();

    // Clear the screen
    let _ = execute!(
        stdout(),
        style::ResetColor,
        terminal::Clear(ClearType::All),
        cursor::Hide,
        cursor::MoveTo(0, 0)
    );
}

/// # reset_terminal
/// Reset the terminal.
fn reset_terminal() {
    io::stdout().flush().unwrap();
    let _ = execute!(stdout(), style::ResetColor, cursor::Show,);
    let _ = terminal::disable_raw_mode();
}

/// # get_command_line_arguments
/// Gets the command line arguments object.  Not included in testing since there are no command lines passed in
#[cfg(not(tarpaulin_include))]
fn get_command_line_arguments() -> Args {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(e) => {
            myprintln!("{}", e);
            myprintln!();
            std::process::exit(-2);
        }
    };
    if args.shared.debug {
        let default_parallelism_approx = num_cpus::get();
        myprintln!("Command: {:?}", args.command);
        myprintln!("Searching for duplicates in: {}", args.shared.path);
        myprintln!(
            "Recursively searching for duplicates: {}",
            args.shared.recursive
        );
        myprintln!("Include empty files: {}", args.shared.include_empty_files);
        myprintln!("Dry run: {}", args.shared.dry_run);
        myprintln!("Include hidden files: {}", args.shared.include_hidden_files);
        myprintln!("Verbose: {}", args.shared.verbose);
        myprintln!("Quiet: {}", args.shared.quiet);
        myprintln!("Wildcard: {}", args.shared.wildcard);
        myprintln!("Exclusion wildcard: {}", args.shared.exclusion_wildcard);
        myprintln!("Available cpus: {}", default_parallelism_approx);
        myprintln!();
    }

    args
}

/// # start_search
/// Start the search for duplicate files.
/// * `file_ops` - The file operations object.
/// * `args` - The command line arguments.
/// * `running` - The running flag.
/// * `Result<SearchResults, io::Error>` - The search results.
/// # Errors
/// * `io::Error` - An error occurred during the search.
fn start_search<T: FileOperations>(
    file_ops: &T,
    args: &Args,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<SearchResults, io::Error> {
    // get the files in the directory
    let folder_path: String = args.shared.path.clone();
    // these are used to pass the running flag to the functions
    let r1 = running.clone();
    let r2 = running.clone();
    let r3 = running.clone();

    // get the files in the directory
    // it calls itself as it traverses the tree if recursive is set
    let result = get_files_in_directory(args, folder_path, None, r1);
    let files = match result {
        Ok(files) => files,
        Err(e) => {
            myprintln!("Error: {}", e);
            return Err(e);
        }
    };
    if args.shared.verbose {
        myprintln!("Found {} files", files.len());
    }

    // identify the duplicates
    let full_hash_map = identify_duplicates(args, files, r2);
    // process the duplicates
    let hash_map = process_duplicates(file_ops, args, &full_hash_map, r3);

    // print the duplicate results
    let duplicates_found = hash_map.len();
    let mut duplicates_total_size: i64 = 0;
    for (hash, files) in hash_map.iter() {
        if args.shared.verbose {
            myprintln!("Found {} duplicates for hash: {}", files.len(), hash);
        }
        for file in files {
            if args.shared.verbose {
                myprintln!(
                    "File: {} [created: {}] [modified: {}] [{} bytes]",
                    file.path,
                    file.created_at.to_rfc2822(),
                    file.modified_at.to_rfc2822(),
                    bytesize::ByteSize(file.size)
                );
            }
            if files.iter().position(|f| f.path == file.path).unwrap() != 0 {
                duplicates_total_size += file.size as i64;
            }
        }
        if args.shared.verbose {
            myprintln!();
        }
    }

    // return the search results
    let search_results: SearchResults = SearchResults {
        number_duplicates: duplicates_found,
        total_size: duplicates_total_size as usize,
    };
    Ok(search_results)
}

/// # get_files_in_directory
/// Get files in the specified directory. Calls itself recursively if the recursive flag is set.
/// * `args` - The command line arguments.
/// * `folder_path` - The directory to search in.
/// * `multi` - The progress bar (optional)
/// * `running` - The running flag.
/// * `Result<Vec<FileInfo>, io::Error>` - The files in the directory.
/// # Errors
/// * `io::Error` - An error occurred during the search.
///
fn get_files_in_directory(
    args: &Args,
    folder_path: String,
    multi: Option<progressbar::MultiProgress>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<Vec<FileInfo>, io::Error> {
    let multi = multi.unwrap_or_else(progressbar::MultiProgress::new);
    let mut files: Vec<FileInfo> = Vec::new();
    let running2 = running.clone();

    if running.load(std::sync::atomic::Ordering::SeqCst) == false {
        return Err(io::Error::new(io::ErrorKind::Other, "Program terminated"));
    }

    // check if the path is a directory
    match fs::metadata(folder_path.as_str()) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                myeprintln!("The path provided {} is not a directory", folder_path);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "The path provided is not a directory",
                ));
            }
        }
        Err(e) => {
            myeprintln!("Error calling fs::metadata with path {}", folder_path);
            return Err(e);
        }
    }
    if args.shared.debug {
        let _ = multi.println(&format!("Collecting objects in: {}", folder_path));
    }

    // collect the entries in the directory
    let entries = fs::read_dir(&folder_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
    if args.shared.debug {
        let _ = multi.println(&format!("Finished collecting objects in: {}", folder_path));
    }

    // only add a spinner if the multi is empty
    let bar = if args.shared.quiet {
        multi.add(progressbar::ProgressBar::hidden())
    } else {
        if multi.get_progress_bars_count() == 0 {
            multi.add_with_location(
                progressbar::ProgressBar::new_spinner().with_message("Processing files..."),
                AddLocation::Bottom,
            )
        } else {
            multi.add(progressbar::ProgressBar::hidden())
        }
    };
    multi.draw_all();
    bar.start_spinner();

    let mut folder_count = 0;
    let mut file_count = 0;
    let mut folders: Vec<PathBuf> = Vec::new();
    let workers = num_cpus::get();
    let pool = ThreadPool::new(workers);
    let (tx, rx) = channel();
    let files_count = entries.len();

    if args.shared.debug {
        let _ = multi.println(&format!("Iterating entries: {}", folder_path));
    }

    // use thread pool to optimize the process of scanning then directory objects
    // if there are a lot of folders and/or files in the directory, this will speed up the process
    for entry in entries.iter() {
        let tx = tx.clone();
        let entry = entry.clone();
        let running = running.clone();
        pool.execute(move || {
            // check if the entry is a directory
            if running.load(std::sync::atomic::Ordering::SeqCst) == false {
                return;
            }
            let is_dir = entry.is_dir();
            tx.send((entry, is_dir)).unwrap_or_default();
        });
    }
    if args.shared.debug {
        let _ = multi.println(&format!("Completed iterating entries: {}", folder_path));
    }

    // wait for the jobs to complete, and process the results
    let mut processed = 0;
    while processed < files_count {
        // check if the program is still running
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(files);
        }
        match rx.try_recv() {
            Ok((entry, is_dir)) => {
                if is_dir {
                    folder_count += 1;
                    folders.push(entry.clone());
                } else {
                    file_count += 1;
                }
                processed += 1;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No message available, yield to other threads
                thread::yield_now();
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                break;
            }
        }
    }

    // check if the program is still running
    if !running.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(files);
    }

    // process the folders
    if folder_count != 0 {
        let bar2 = if args.shared.quiet {
            multi.add(progressbar::ProgressBar::hidden())
        } else {
            multi.add(progressbar::ProgressBar::new(folder_count))
        };
        multi.draw_all();

        for fld in folders.iter() {
            multi.set_message(&bar2, format!("Folder {}", fld.display()).as_str());
            let hidden: bool;
            // check if the folder is hidden - use appropriate code for the OS
            #[cfg(not(target_os = "windows"))]
            {
                hidden = fld.file_name().unwrap().to_str().unwrap().starts_with(".");
            }
            #[cfg(target_os = "windows")]
            {
                if std::fs::metadata(&path)
                    .unwrap()
                    .file_attributes()
                    .hidden()
                    .unwrap()
                {
                    hidden = true;
                }
            }

            if hidden {
                if args.shared.include_hidden_files == false {
                    if args.shared.verbose {
                        let _ = multi.println(&format!(
                            "Ignoring hidden directory: {}",
                            fld.file_name().unwrap().to_str().unwrap()
                        ));
                    }
                    multi.increment(&bar2, 1);
                    continue;
                }
            }

            // if we aren't recursive, then ignore any folders we find
            if !args.shared.recursive {
                if args.shared.verbose {
                    let _ = multi.println(&format!(
                        "Ignoring directory: {}",
                        fld.file_name().unwrap().to_str().unwrap()
                    ));
                }
                multi.increment(&bar2, 1);
            } else {
                // if we are recursive, then process the sub folders
                let path = fld.as_path();
                // recursion call
                let sub_files = get_files_in_directory(
                    args,
                    path.to_str().unwrap().to_string(),
                    Some(multi.clone()),
                    running2.clone(),
                )?;
                // add results to our files vector
                files.extend(sub_files);
                multi.increment(&bar2, 1);
            }
        }

        // remove the progress bar for this folder
        bar2.finish();
        multi.remove(&bar2);
    }

    // now process files
    if file_count != 0 {
        let bar2 = if args.shared.quiet {
            multi.add(progressbar::ProgressBar::hidden())
        } else {
            multi.add(progressbar::ProgressBar::new(file_count))
        };
        multi.draw_all();

        for entry in entries.iter() {
            // check if the program is still running
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let path = entry.as_path();
            let _ = multi.set_message(&bar2, format!("Processing: {}", path.display()).as_str());

            if path.is_file() {
                // determine if the file matches the wildcard
                let wildcard_pattern = glob::Pattern::new(&args.shared.wildcard)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                if !wildcard_pattern.matches_path(path) {
                    if args.shared.verbose {
                        let _ = multi.println(&format!(
                            "Ignoring file (does not match wildcard): {}",
                            path.to_str().unwrap()
                        ));
                    }
                    multi.increment(&bar2, 1);
                    continue;
                }
                // determine if the file matches the exclusion wildcard
                if args.shared.exclusion_wildcard.len() > 0 {
                    let exclusion_wildcard_pattern =
                        glob::Pattern::new(&args.shared.exclusion_wildcard)
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                    if exclusion_wildcard_pattern.matches_path(path) {
                        if args.shared.verbose {
                            let _ = multi.println(&format!(
                                "Ignoring file (matches exclusion wildcard): {}",
                                path.to_str().unwrap()
                            ));
                        }
                        bar2.increment(1);
                        continue;
                    }
                }

                // check if file is hidden using appropriate code for the OS
                let hidden: bool;
                #[cfg(not(target_os = "windows"))]
                {
                    hidden = path.file_name().unwrap().to_str().unwrap().starts_with(".");
                }
                #[cfg(target_os = "windows")]
                {
                    if std::fs::metadata(&path)
                        .unwrap()
                        .file_attributes()
                        .hidden()
                        .unwrap()
                    {
                        hidden = true;
                    }
                }
                if args.shared.include_hidden_files == false && hidden {
                    // skip hidden files if not including them
                    if args.shared.verbose {
                        let _ = multi
                            .println(&format!("Ignoring hidden file: {}", path.to_str().unwrap()));
                    }

                    multi.increment(&bar2, 1);
                    continue;
                }
                // get the file metadata
                let meta = std::fs::metadata(&path).unwrap();
                let size = meta.len();
                if size == 0 && !args.shared.include_empty_files {
                    // skip empty files if not including them
                    if args.shared.verbose {
                        let _ = multi
                            .println(&format!("Ignoring empty file: {}", path.to_str().unwrap()));
                    }

                    multi.increment(&bar2, 1);
                    continue;
                }

                let created_at = meta.created().unwrap();
                let modified_at = meta.modified().unwrap();
                // Convert SystemTime to chrono::DateTime<Utc>
                let created_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                    + chrono::Duration::from_std(created_at.duration_since(UNIX_EPOCH).unwrap())
                        .unwrap();
                let modified_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                    + chrono::Duration::from_std(modified_at.duration_since(UNIX_EPOCH).unwrap())
                        .unwrap();

                // store results in our files vector
                let file_info = FileInfo {
                    path: path.to_str().unwrap().to_string(),
                    size,
                    created_at: created_at_utc_datetime,
                    modified_at: modified_at_utc_datetime,
                };
                files.push(file_info);

                if args.shared.debug {
                    let _ = multi.println(&format!(
                        "Selected File: {} [created: {}] [modified: {}] [{} bytes]",
                        path.to_str().unwrap(),
                        created_at_utc_datetime.to_rfc2822(),
                        modified_at_utc_datetime.to_rfc2822(),
                        size
                    ));
                }
                multi.increment(&bar2, 1);
            }
        }

        bar2.finish();
        multi.remove(&bar2);
    }

    bar.finish();
    multi.remove(&bar);
    Ok(files)
}

/// # identify_duplicates
/// Identify duplicate files based on their MD5 hash
/// * `args` - The command line arguments.
/// * `files` - The files to process.
/// * `running` - The running flag.
fn identify_duplicates(
    args: &Args,
    files: Vec<FileInfo>,
    _running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> HashMap<String, Vec<FileInfo>> {
    let mut hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
    let multi = progressbar::MultiProgress::new();
    let workers = num_cpus::get();

    let bar2 = if args.shared.quiet {
        multi.add(progressbar::ProgressBar::hidden())
    } else {
        multi.add_with_location(
            progressbar::ProgressBar::new_spinner().with_message("Identifying duplicates..."),
            AddLocation::Bottom,
        )
    };

    let bar = if args.shared.quiet {
        multi.add(progressbar::ProgressBar::hidden())
    } else {
        multi.add(progressbar::ProgressBar::new(
            files.len().try_into().unwrap(),
        ))
    };

    multi.draw_all();
    bar2.start_spinner();

    // we will use a thread pool to optimize the hashing process
    // the thread pool will use one thread per cpu core

    let pool = ThreadPool::new(workers);
    let (tx, rx) = channel();
    let files_count = files.len();

    // setup our jobs for the thread pool
    for file in files {
        let tx = tx.clone();
        let bar = bar.clone();
        let file_path = file.path.clone();

        let bar_clone = bar.clone();
        pool.execute(move || {
            let hash_result = get_hash_of_file(&file_path, &bar_clone);
            // handle an error
            match hash_result {
                Ok(hash_string) => tx.send((hash_string, file.clone())).unwrap(),
                Err(e) => {
                    myeprintln!("{}", e);
                    return tx.send((String::new(), file.clone())).unwrap();
                }
            }
        });
    }

    // wait for the jobs to complete, and process the results
    rx.iter().take(files_count).for_each(|(hash_string, file)| {
        if hash_string.is_empty() {
            if args.shared.debug {
                let _ = multi.println(&format!(
                    "File: {} [{} bytes] [error calculating hash]",
                    file.path, file.size
                ));
            }
            return;
        }
        if args.shared.verbose {
            let _ = multi.println(&format!(
                "File: {} [{} bytes] [hash: {}]",
                file.path, file.size, hash_string
            ));
        }
        // add the file and hash to the map
        // if the hash doesn't exist, create a new vector
        if !hash_map.contains_key(&hash_string) {
            let mut vec = Vec::new();
            vec.push(file);
            hash_map.insert(hash_string.to_string(), vec);
        } else {
            let vec = hash_map.get_mut(&hash_string).unwrap();
            vec.push(file);
        }
        multi.increment(&bar, 1);
    });

    let _ = bar.finish();
    let _ = bar2.finish();

    multi.remove(&bar2);
    multi.remove(&bar);
    multi.finish_all();

    hash_map
}

/// # process_duplicates
/// Process the duplicate files using the method specified in cmd line args
/// * `file_ops` - The file operations object.
/// * `args` - The command line arguments.
/// * `hash_map` - The hash map of files.
/// * `running` - The running flag.
fn process_duplicates<T: FileOperations>(
    file_ops: &T,
    args: &Args,
    hash_map: &HashMap<String, Vec<FileInfo>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> HashMap<String, Vec<FileInfo>> {
    let mut new_hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();

    let mut multi = progressbar::MultiProgress::new();

    let bar2 = if args.shared.quiet {
        multi.add(progressbar::ProgressBar::hidden())
    } else {
        multi.add_with_location(
            progressbar::ProgressBar::new_spinner().with_message("Processing duplicates..."),
            AddLocation::Bottom,
        )
    };

    let bar = if args.shared.quiet {
        multi.add(progressbar::ProgressBar::hidden())
    } else {
        multi.add(progressbar::ProgressBar::new(
            hash_map.len().try_into().unwrap(),
        ))
    };

    multi.draw_all();
    bar2.start_spinner();

    // get the method
    let method = match &args.command {
        Commands::MoveDuplicates { method, .. } => method,
        Commands::CopyDuplicates { method, .. } => method,
        Commands::DeleteDuplicates { method } => method,
        Commands::FindDuplicates { method } => method,
    };

    // if the duplicate selection method is "interactive" then we need to turn off the progress bars
    if *method == DuplicateSelectionMethod::Interactive {
        let _ = bar.finish();
        let _ = bar2.finish();
        multi.remove(&bar2);
        multi.remove(&bar);
        multi.finish_all();
    }

    // get list of files to process
    for (hash, files) in hash_map.iter() {
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        // if there is only one file, then it isn't a duplicate
        if files.len() > 1 {
            new_hash_map.insert(hash.clone(), files.clone());

            if args.shared.debug {
                match &args.command {
                    Commands::FindDuplicates { method: _ } => {
                        let _ = multi.println(&format!("FindDuplicates: {}", hash));
                    }
                    Commands::MoveDuplicates {
                        location,
                        method: _,
                    } => {
                        let _ = multi.println(&format!("MoveDuplicates: {} to {}", hash, location));
                    }
                    Commands::CopyDuplicates {
                        location,
                        method: _,
                    } => {
                        let _ = multi.println(&format!("CopyDuplicates: {} to {}", hash, location));
                    }
                    Commands::DeleteDuplicates { method: _ } => {
                        let _ = multi.println(&format!("DeleteDuplicates: {}", hash));
                    }
                }
            }

            if let Commands::FindDuplicates { .. } = args.command {
                continue;
            }

            let dup_fileset = select_duplicate_files(method.clone(), hash, files, &bar2);
            if dup_fileset.keeper.is_none() {
                if args.shared.debug {
                    let _ = multi.eprintln("**************************************");
                    let _ = multi.eprintln("keeper is none, this shouldn't happen!");
                    let _ = multi.eprintln(&format!("Method: {:?}", method));
                    let _ = multi.eprintln(&format!("Files: {:?}", files));
                    let _ = multi.eprintln("**************************************");
                }
                continue;
            }
            if args.shared.debug {
                let _ = multi.println(&format!(
                    "Selected File: {}",
                    dup_fileset.keeper.unwrap().path
                ));
            }

            for file in dup_fileset.extras {
                let _ = process_a_duplicate_file(file_ops, args, &file, &mut multi);
            }
        }
        multi.increment(&bar, 1);
    }

    let _ = bar.finish();
    let _ = bar2.finish();
    multi.remove(&bar2);
    multi.remove(&bar);
    multi.finish_all();
    new_hash_map
}

/// # process_a_duplicate_file
/// Process a duplicate file based on the command line arguments
/// * `file_ops` - The file operations object.
/// * `args` - The command line arguments.
/// * `file` - The file to process.
/// * `multi` - The progress bar.
/// * `Result<(), std::io::Error>` - The result of the operation.
/// # Errors
/// * `std::io::Error` - An error occurred during the operation.
fn process_a_duplicate_file<T: FileOperations>(
    file_ops: &T,
    args: &Args,
    file: &FileInfo,
    multi: &mut progressbar::MultiProgress,
) -> Result<(), std::io::Error> {
    let source = &file.path;
    let file_name = Path::new(&file.path).file_name().unwrap().to_str().unwrap();
    let location = match &args.command {
        Commands::MoveDuplicates { location, .. } => location,
        Commands::CopyDuplicates { location, .. } => location,
        Commands::DeleteDuplicates { method: _ } => "",
        Commands::FindDuplicates { method: _ } => "",
    };
    let destination = format!("{}/{}", location, file_name);

    let command_text: String;
    let mut error: Option<std::io::Error> = None;

    match args.command {
        Commands::FindDuplicates { .. } => {
            command_text = "Find".to_string();
        }
        Commands::MoveDuplicates { .. } => {
            command_text = "Move".to_string();
        }
        Commands::CopyDuplicates { .. } => {
            command_text = "Copy".to_string();
        }
        Commands::DeleteDuplicates { .. } => {
            command_text = "Delete".to_string();
        }
    }

    // if not a dry run, then perform the operation
    if !args.shared.dry_run {
        if args.shared.verbose {
            // location is empty for Find and Delete commands
            if location.is_empty() {
                let _ = multi.println(&format!("{}ing: {}", command_text, source));
            } else {
                let _ = multi.println(&format!(
                    "{}ing: {} to {}",
                    command_text, source, destination
                ));
            }
        }

        match args.command {
            Commands::FindDuplicates { .. } => {}
            Commands::MoveDuplicates { .. } => {
                if let Err(result) = file_ops.rename(source, &destination) {
                    error = Some(result);
                }
            }
            Commands::CopyDuplicates { .. } => {
                if let Err(result) = file_ops.copy(source, &destination) {
                    error = Some(result);
                }
            }
            Commands::DeleteDuplicates { .. } => {
                if let Err(result) = file_ops.remove_file(source) {
                    error = Some(result);
                }
            }
        }

        if error.is_some() {
            let _ = multi.println(&format!(
                "*** Failed to {} {} to {}: {:?}",
                command_text, source, destination, error
            ));
        }
    } else {
        if args.shared.verbose {
            let _ = multi.println(&format!(
                "Dry run: Would {} {} to {}",
                command_text, source, destination
            ));
        }
    }
    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// # get_hash_of_file
/// Get the MD5 hash of a file
/// * `file_path` - The path to the file.
/// * `bar` - The progress bar.
/// * `Result<String, std::io::Error>` - The MD5 hash of the file.
/// # Errors
/// * `std::io::Error` - An error occurred during the operation.
fn get_hash_of_file(
    file_path: &str,
    _bar: &progressbar::ProgressBar,
) -> Result<String, std::io::Error> {
    let result = std::fs::File::open(file_path);
    match result {
        Ok(_) => {
            let mut file = std::fs::File::open(file_path).unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            Ok(get_md5_hash(&buffer))
        }
        Err(e) => {
            myeprintln!("{}", format!("{:?}", e));
            Err(e)
        }
    }
}

/// # get_md5_hash
/// Get the MD5 hash of a buffer
/// * `buffer` - The buffer to hash.
/// # Returns
/// * `String` - The MD5 hash.
fn get_md5_hash(buffer: &Vec<u8>) -> String {
    let mut hasher = md5::Md5::new();
    hasher.update(&buffer);
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

/// # select_duplicate_files
/// Select the duplicate files based on the method specified in the command line arguments
/// * `method` - The method to use.
/// * `hash` - The hash of the files.
/// * `files` - The files to process.
/// * `bar` - The progress bar.
/// # Returns
/// * `DuplicateFileSet` - The set of duplicate files.
fn select_duplicate_files(
    method: DuplicateSelectionMethod,
    hash: &String,
    files: &Vec<FileInfo>,
    _bar: &progressbar::ProgressBar,
) -> DuplicateFileSet {
    let mut dup_fileset = DuplicateFileSet {
        keeper: None,
        extras: vec![],
    };
    if files.is_empty() {
        return dup_fileset;
    }
    match method {
        DuplicateSelectionMethod::Newest => {
            // keep the newest file, so return all other files
            let mut sorted_files = files.clone();
            sorted_files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
            let keeper = sorted_files.swap_remove(0);
            dup_fileset.keeper = Some(keeper);
            dup_fileset.extras = sorted_files;
        }
        DuplicateSelectionMethod::Oldest => {
            // keep the oldest file, so return all other files
            let mut sorted_files = files.clone();
            sorted_files.sort_by(|a, b| a.modified_at.cmp(&b.modified_at));
            let keeper = sorted_files.swap_remove(0);
            dup_fileset.keeper = Some(keeper);
            dup_fileset.extras = sorted_files;
        }
        DuplicateSelectionMethod::Interactive => {
            use crossterm::execute;
            let mut selected_index = 0;

            let _ = queue!(
                stdout(),
                SetAttribute(style::Attribute::Bold),
                style::Print("Duplicate File Interactive Selector"),
                cursor::MoveToNextLine(1),
                SetAttribute(style::Attribute::Reset),
                style::Print(""),
                cursor::MoveToNextLine(1),
                style::Print("Move the selector up and down using the ARROW keys"),
                cursor::MoveToNextLine(1),
                style::Print("Press SPACE to select one or more files to keep"),
                cursor::MoveToNextLine(1),
                style::Print("Press ENTER to process the unselected duplicate files and continue"),
                cursor::MoveToNextLine(1),
                style::Print("Press ESC to exit"),
                cursor::MoveToNextLine(1),
                style::Print(""),
                cursor::MoveToNextLine(1),
                style::Print(format!("For hash [{}]:", hash)),
                cursor::MoveToNextLine(1),
            );

            loop {
                let _ = queue!(stdout(), style::Print(""), cursor::MoveToNextLine(1));
                // print out list of files to the user
                for (i, item) in files.iter().enumerate() {
                    if i == selected_index {
                        let _ = queue!(
                            stdout(),
                            SetForegroundColor(Color::Red),
                            style::Print(format!("> {}", item.path)),
                            cursor::MoveToNextLine(1)
                        );
                    } else {
                        let _ = queue!(
                            stdout(),
                            SetForegroundColor(Color::Red),
                            style::Print(format!("  {}", item.path)),
                            cursor::MoveToNextLine(1)
                        );
                    }
                }
                let _ = queue!(
                    stdout(),
                    cursor::MoveToPreviousLine((1 + files.len()).try_into().unwrap(),)
                );
                let _ = stdout().flush();

                // get key events

                if let Event::Key(KeyEvent { code, .. }) = event::read().unwrap() {
                    match code {
                        KeyCode::Up => {
                            if selected_index > 0 {
                                selected_index -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if selected_index < files.len() - 1 {
                                selected_index += 1;
                            }
                        }
                        KeyCode::Enter => {
                            break;
                        }
                        KeyCode::Esc => {
                            let _ = execute!(
                                stdout(),
                                style::ResetColor,
                                cursor::Show,
                                terminal::LeaveAlternateScreen
                            );
                            let _ = terminal::disable_raw_mode();
                            return dup_fileset;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    dup_fileset
}

/// # Tests
///
/// Unit tests for the various functions and features of the program.
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // setup mock file operations

    struct MockFileOperationsOk;

    impl FileOperations for MockFileOperationsOk {
        fn copy(&self, _source: &str, _destination: &str) -> Result<(), std::io::Error> {
            // Mock implementation
            Ok(())
        }

        fn remove_file(&self, _source: &str) -> Result<(), std::io::Error> {
            // Mock implementation
            Ok(())
        }

        fn rename(&self, _source: &str, _destination: &str) -> Result<(), std::io::Error> {
            // Mock implementation
            Ok(())
        }
    }

    struct MockFileOperationsError;

    impl FileOperations for MockFileOperationsError {
        fn copy(&self, _source: &str, _destination: &str) -> Result<(), std::io::Error> {
            // Mock implementation - produce an error
            Err(io::Error::new(io::ErrorKind::Other, "Mock error"))
        }

        fn remove_file(&self, _source: &str) -> Result<(), std::io::Error> {
            // Mock implementation - produce an error
            Err(io::Error::new(io::ErrorKind::Other, "Mock error"))
        }

        fn rename(&self, _source: &str, _destination: &str) -> Result<(), std::io::Error> {
            // Mock implementation - produce an error
            Err(io::Error::new(io::ErrorKind::Other, "Mock error"))
        }
    }

    fn create_default_command_line_arguments() -> Args {
        let shared_options = SharedOptions {
            path: "testdata".to_string(),
            recursive: false,
            debug: true,
            include_empty_files: false,
            dry_run: true,
            include_hidden_files: false,
            verbose: true,
            quiet: false,
            wildcard: "*".to_string(),
            exclusion_wildcard: "".to_string(),
        };
        let s1 = shared_options.clone();
        let args = Args {
            shared: s1,
            command: Commands::FindDuplicates {
                method: DuplicateSelectionMethod::Newest,
            },
        };
        args
    }

    #[test]
    fn test_get_files_in_directory() {
        let args = create_default_command_line_arguments();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files_in_directory_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.quiet = true;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files_in_directory_wildcard() {
        let mut args = create_default_command_line_arguments();
        args.shared.wildcard = "*testdupe*.txt".to_string();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 4);
    }

    #[test]
    fn test_get_files_in_directory_exclusion_wildcard() {
        let mut args = create_default_command_line_arguments();
        args.shared.exclusion_wildcard = "*testdupe*.txt".to_string();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_get_files_in_directory_include_empty() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_empty_files = true;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 7);
    }

    #[test]
    fn test_get_files_in_directory_include_hidden() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_hidden_files = true;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 6);
    }

    #[test]
    fn test_get_files_in_directory_include_all_files() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_hidden_files = true;
        args.shared.include_empty_files = true;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 8);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 16);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive_with_hidden() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        args.shared.include_hidden_files = true;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r).unwrap();
        assert_eq!(files.len(), 19);
    }

    #[test]
    fn test_get_files_in_directory_bad_path() {
        let mut args = create_default_command_line_arguments();
        args.shared.path = "badpath!!!".to_string();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let result = get_files_in_directory(&args, "badpath!!!".to_string(), None, r);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_files_in_directory_notafolder() {
        let args = create_default_command_line_arguments();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let result = get_files_in_directory(
            &args,
            format!("{}/testnodupe.txt", args.shared.path),
            None,
            r,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_get_hash_of_file() {
        let args = create_default_command_line_arguments();
        let hash = get_hash_of_file(
            &format!("{}//testdupe1.txt", args.shared.path.clone()),
            &progressbar::ProgressBar::new_spinner().with_message("none"),
        );
        assert!(hash.is_ok());
        assert_eq!(hash.unwrap(), "8c91214730e59f67bd46d1855156e762");
    }

    #[test]
    fn test_get_hash_of_file_bad_path() {
        let args = create_default_command_line_arguments();
        let hash = get_hash_of_file(
            &format!("{}//testdupe1-notfound.txt", args.shared.path.clone()),
            &progressbar::ProgressBar::new_spinner().with_message("none"),
        );
        assert!(hash.is_err());
    }

    #[test]
    fn test_get_md5_hash() {
        let buffer = "Hello, world!".as_bytes().to_vec();
        let hash = get_md5_hash(&buffer);
        assert_eq!(hash, "6cd3556deb0da54bca060b4c39479839");
    }

    #[test]
    fn test_get_md5_hash_empty() {
        let buffer = "".as_bytes().to_vec();
        let hash = get_md5_hash(&buffer);
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_start_search() {
        let args = create_default_command_line_arguments();
        let file_ops = RealFileOperations;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let result = start_search(&file_ops, &args, running);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.quiet = true;
        let file_ops = RealFileOperations;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let result = start_search(&file_ops, &args, running);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_badpath() {
        let mut args = create_default_command_line_arguments();
        args.shared.path = "badpath".to_owned();
        args.shared.recursive = true;
        args.shared.dry_run = true;
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap().to_string();
        println!("Temporary location : {}", temp_path);
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let file_ops = RealFileOperations;
        let result = start_search(&file_ops, &args, r);
        assert!(result.is_err());
    }

    #[test]
    fn test_start_search_copy() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        args.shared.dry_run = true;
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap().to_string();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        println!("Temporary location : {}", temp_path);

        args.command = Commands::CopyDuplicates {
            location: temp_path,
            method: DuplicateSelectionMethod::Newest,
        };
        let file_ops = RealFileOperations;
        let result = start_search(&file_ops, &args, r);

        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_bad_path() {
        let mut args = create_default_command_line_arguments();
        args.shared.path = "data-badpath!!!".to_string();
        let file_ops = RealFileOperations;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let result = start_search(&file_ops, &args, r);
        assert!(result.is_err());
    }

    #[test]
    fn test_start_search_nodupes() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        args.shared.dry_run = true;
        args.shared.wildcard = "testnodupe.txt".to_owned();
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap().to_string();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        println!("Temporary location : {}", temp_path);

        args.command = Commands::CopyDuplicates {
            location: temp_path,
            method: DuplicateSelectionMethod::Newest,
        };
        let file_ops = RealFileOperations;
        let result = start_search(&file_ops, &args, r);

        assert!(result.is_ok());
    }

    #[test]
    fn test_identify_duplicates() {
        let args = create_default_command_line_arguments();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r1 = running.clone();
        let r2 = running.clone();

        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r1).unwrap();
        let hash_map = identify_duplicates(&args, files, r2);
        // duplicates are entries in hash_map with more than 1 file
        let mut duplicates_found = 0;
        for (_hash, files) in hash_map.iter() {
            if files.len() > 1 {
                duplicates_found += 1;
            }
        }
        assert_eq!(duplicates_found, 1);
    }

    #[test]
    fn test_identify_duplicates_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.quiet = true;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r1 = running.clone();
        let r2 = running.clone();

        let files = get_files_in_directory(&args, args.shared.path.clone(), None, r1).unwrap();
        let hash_map = identify_duplicates(&args, files, r2);
        // duplicates are entries in hash_map with more than 1 file
        let mut duplicates_found = 0;
        for (_hash, files) in hash_map.iter() {
            if files.len() > 1 {
                duplicates_found += 1;
            }
        }
        assert_eq!(duplicates_found, 1);
    }

    #[test]
    fn test_identify_duplicates_no_files() {
        let args = create_default_command_line_arguments();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();

        let files = Vec::new();
        let hash_map = identify_duplicates(&args, files, r);
        // duplicates are entries in hash_map with more than 1 file
        let mut duplicates_found = 0;
        for (_hash, files) in hash_map.iter() {
            if files.len() > 1 {
                duplicates_found += 1;
            }
        }
        assert_eq!(duplicates_found, 0);
    }

    #[test]
    fn test_identify_duplicates_badfiles() {
        let args = create_default_command_line_arguments();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        let mut files = Vec::new();
        let file = FileInfo {
            path: "todo!()".to_owned(),
            size: 123,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        files.push(file);
        let hash_map = identify_duplicates(&args, files, r);
        // duplicates are entries in hash_map with more than 1 file
        let mut duplicates_found = 0;
        for (_hash, files) in hash_map.iter() {
            if files.len() > 1 {
                duplicates_found += 1;
            }
        }
        assert_eq!(duplicates_found, 0);
    }

    #[test]
    fn test_select_duplicate_files_newest() {
        let args = create_default_command_line_arguments();
        let mut files = Vec::new();
        files.push(FileInfo {
            path: format!("{}//testdupe1.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        });
        files.push(FileInfo {
            path: format!("{}//testdupe2.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now() - chrono::Duration::days(1),
            modified_at: Utc::now() - chrono::Duration::days(1),
        });
        files.push(FileInfo {
            path: format!("{}//testdupe3.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now() - chrono::Duration::days(2),
            modified_at: Utc::now() - chrono::Duration::days(2),
        });
        let bar = progressbar::ProgressBar::new_spinner().with_message("none");
        let dup_fileset = select_duplicate_files(
            DuplicateSelectionMethod::Newest,
            &"testhash".to_owned(),
            &files,
            &bar,
        );
        assert_eq!(dup_fileset.keeper.is_some(), true);
        assert_eq!(
            dup_fileset.keeper.unwrap().path,
            format!("{}//testdupe1.txt", args.shared.path.clone())
        );
        assert_eq!(dup_fileset.extras.len(), 2);
        // the order of the selected files is not guarenteed, so we check to see if our files are just in there somewhere
        let file1 = dup_fileset
            .extras
            .iter()
            .find(|file| file.path == format!("{}//testdupe3.txt", args.shared.path.clone()));
        let file2 = dup_fileset
            .extras
            .iter()
            .find(|file| file.path == format!("{}//testdupe2.txt", args.shared.path.clone()));

        assert!(file1.is_some());
        assert!(file2.is_some());
    }

    #[test]
    fn test_select_duplicate_files_oldest() {
        let args = create_default_command_line_arguments();
        let mut files = Vec::new();
        files.push(FileInfo {
            path: format!("{}//testdupe1.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        });
        files.push(FileInfo {
            path: format!("{}//testdupe2.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now() - chrono::Duration::days(1),
            modified_at: Utc::now() - chrono::Duration::days(1),
        });
        files.push(FileInfo {
            path: format!("{}//testdupe3.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now() - chrono::Duration::days(2),
            modified_at: Utc::now() - chrono::Duration::days(2),
        });
        let bar = progressbar::ProgressBar::new_spinner().with_message("none");
        let dup_fileset = select_duplicate_files(
            DuplicateSelectionMethod::Oldest,
            &"testhash".to_owned(),
            &files,
            &bar,
        );
        assert_eq!(dup_fileset.keeper.is_some(), true);
        assert_eq!(
            dup_fileset.keeper.unwrap().path,
            format!("{}//testdupe3.txt", args.shared.path.clone()),
        );
        assert_eq!(dup_fileset.extras.len(), 2);

        // the order of the selected files is not guarenteed, so we check to see if our files are just in there somewhere
        let file1 = dup_fileset
            .extras
            .iter()
            .find(|file| file.path == format!("{}//testdupe1.txt", args.shared.path.clone()));
        let file2 = dup_fileset
            .extras
            .iter()
            .find(|file| file.path == format!("{}//testdupe2.txt", args.shared.path.clone()));

        assert!(file1.is_some());
        assert!(file2.is_some());
    }

    #[test]
    fn test_select_duplicate_files_empty_files() {
        let files = Vec::new();
        let bar = progressbar::ProgressBar::new_spinner().with_message("none");
        let dup_fileset = select_duplicate_files(
            DuplicateSelectionMethod::Oldest,
            &"testhash".to_owned(),
            &files,
            &bar,
        );
        assert_eq!(dup_fileset.keeper.is_none(), true);
        assert_eq!(dup_fileset.extras.len(), 0);
    }

    #[test]
    fn test_process_a_duplicate_file_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators - returns ok for file operations
        let file_ops = MockFileOperationsOk;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        // FindCommand does not operate on the file, so it always returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_file_find() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators - returns ok for file operations
        let file_ops = MockFileOperationsOk;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        // FindCommand does not operate of the file, so it always returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_file_find_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.shared.quiet = true;
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators - returns ok for file operations
        let file_ops = MockFileOperationsOk;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        // FindCommand does not operate of the file, so it always returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_delete_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::DeleteDuplicates {
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsError;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_a_duplicate_delete() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::DeleteDuplicates {
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsOk;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_copy_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::CopyDuplicates {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsError;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_a_duplicate_copy() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::CopyDuplicates {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsOk;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_move_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::MoveDuplicates {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsError;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_a_duplicate_move() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::MoveDuplicates {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = progressbar::MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsOk;
        let result = process_a_duplicate_file(&file_ops, &args, &file_info, &mut multi);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_duplicates_move() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::MoveDuplicates {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
        };
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();

        // create a fake hash map
        let mut hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
        // fake files
        let mut files: Vec<FileInfo> = Vec::new();
        files.push(FileInfo {
            path: format!("{}//testdupe1.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        });
        files.push(FileInfo {
            path: format!("{}//testdupe2.txt", args.shared.path.clone()),
            size: 1024,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        });
        hash_map.insert("testhashkey".to_owned(), files);

        // use our mock file operators
        let file_ops = MockFileOperationsOk;
        let result = process_duplicates(&file_ops, &args, &hash_map, r);
        println!("{:?}", result);
        assert_eq!(result.contains_key("testhashkey"), true);
    }
}
