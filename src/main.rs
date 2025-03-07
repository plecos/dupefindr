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
use dialoguer_ext::console::{style, Key};
use dialoguer_ext::theme::ColorfulTheme;
use dialoguer_ext::Select;
use errors::{InteractiveError, InteractiveErrorKind};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use md5::{self, Digest};
use std::collections::HashMap;
use std::io::{self, Read};
#[cfg(target_os = "windows")]
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::thread::yield_now;
use std::time::UNIX_EPOCH;
use std::time::{Duration, Instant};
use std::{fs, thread};
use threadpool::ThreadPool;

mod errors;

const BUFFER_READ_SIZE: usize = 1024 * 1024;

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

    /// Max threads to use
    /// Example: 4
    /// Default: Number of CPUs
    /// If set to 0, then it will use the number of CPUs
    #[arg(short, long, default_value = "0")]
    max_threads: Option<usize>,

    /// Create a report
    #[arg(long, default_value = "false")]
    create_report: bool,

    /// Path of the report
    /// Defaults to the folder where dupefindr was run
    #[arg(long, default_value = "./dupefindr-report.csv")]
    report_path: String,
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

#[derive(Subcommand, Debug, PartialEq, Clone)]
enum Commands {
    #[command(name = "find", about = "Find duplicate files")]
    Find {
        /// Method to select the file to keep
        /// Example: newest, oldest, largest, smallest
        #[arg(short, long, default_value = "newest")]
        method: DuplicateSelectionMethod,
    },

    #[command(name = "move", about = "Move duplicate files to a new location")]
    Move {
        /// The directory to move to.
        #[arg(short, long)]
        location: String,

        /// Method to select the file to keep
        /// Example: newest, oldest, largest, smallest
        #[arg(short, long, default_value = "newest")]
        method: DuplicateSelectionMethod,

        // do not create subdirectories in the destination
        #[arg(short, long, default_value = "false")]
        flatten: bool,

        // do not add the hash of thie file as a folder in the destination and group the duplicates in that folder
        #[arg(short, long, default_value = "false")]
        no_hash_folder: bool,

        // overwrite the destination file if it exists - this includes any duplicates that are copied that have the same name
        #[arg(short, long, default_value = "false")]
        overwrite: bool,
    },
    #[command(name = "copy", about = "Copy duplicate files to a new location")]
    Copy {
        /// The directory to copy to.
        #[arg(short, long)]
        location: String,

        /// Method to select the file to keep
        #[arg(short, long, default_value = "newest")]
        method: DuplicateSelectionMethod,

        // do not create subdirectories in the destination
        #[arg(short, long, default_value = "false")]
        flatten: bool,

        // do not add the hash of thie file as a folder in the destination and group the duplicates in that folder
        #[arg(short, long, default_value = "false")]
        no_hash_folder: bool,

        // overwrite the destination file if it exists - this includes any duplicates that are copied that have the same name
        #[arg(short, long, default_value = "false")]
        overwrite: bool,
    },
    #[command(name = "delete", about = "Delete duplicate files")]
    Delete {
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

/// # DuplicateResult
/// Specifies the result of the duplication action
/// * `Skipped` - the duplicates were left as is
/// * `Deleted` - the duplicates were deleted
/// * `Copied` - the duplicates were copied
/// * `Moved` - the duplicates were moved
/// * `Found` - the duplicates were found, and left as is
/// * `Aborted` - user aborted the duplication processing
#[derive(Debug, Clone, PartialEq)]
enum DuplicateResult {
    Skipped,
    Deleted,
    Copied,
    Moved,
    Found,
    Aborted,
}

/// # DuplicateFileSet
///
/// Struct representing a set of duplicate files.
///
/// * `keeper` - The file to keep.
/// * `extras` - The duplicate files.
/// * `result` - What happened to the duplicate files
#[derive(Debug, Clone)]
struct DuplicateFileSet {
    hash: String,
    keeper: Option<FileInfo>,
    extras: Vec<FileInfo>,
    result: DuplicateResult,
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
    fn copy(&self, source: &str, destination: &str, overwrite: bool) -> Result<(), std::io::Error>;
    fn remove_file(&self, source: &str) -> Result<(), std::io::Error>;
    fn rename(
        &self,
        source: &str,
        destination: &str,
        overwrite: bool,
    ) -> Result<(), std::io::Error>;
}

/// # RealFileOperations
/// Implementation of `FileOperations` for real file operations.
/// * `copy` - Copy a file from source to destination.
/// * `remove_file` - Remove a file.
/// * `rename` - Rename a file.
struct RealFileOperations;

impl FileOperations for RealFileOperations {
    #[cfg(not(tarpaulin_include))]
    fn copy(&self, source: &str, destination: &str, overwrite: bool) -> Result<(), std::io::Error> {
        let mut counter = 1;
        let mut new_destination = destination.to_string();
        // if overwrite is false,
        // then if the destination file already exists, then add a counter to the filename
        if !overwrite {
            loop {
                match std::path::Path::new(&new_destination).try_exists() {
                    Ok(flag) => {
                        if !flag {
                            break;
                        }
                        let path = std::path::Path::new(destination);
                        let parent = path.parent().unwrap().to_str().unwrap();
                        let file_stem = path.file_stem().unwrap().to_str().unwrap();
                        let extension = path.extension().unwrap_or_default().to_str().unwrap();
                        new_destination =
                            format!("{}/{}_{}.{}", parent, file_stem, counter, extension);
                        counter += 1;
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }
        // copy the file
        match std::fs::copy(source, &new_destination) {
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
    fn rename(
        &self,
        source: &str,
        destination: &str,
        overwrite: bool,
    ) -> Result<(), std::io::Error> {
        let mut counter = 1;
        let mut new_destination = destination.to_string();
        // if overwrite is false,
        // then if the destination file already exists, then add a counter to the filename
        if !overwrite {
            loop {
                match std::path::Path::new(&new_destination).try_exists() {
                    Ok(flag) => {
                        if !flag {
                            break;
                        }
                        let path = std::path::Path::new(destination);
                        let parent = path.parent().unwrap().to_str().unwrap();
                        let file_stem = path.file_stem().unwrap().to_str().unwrap();
                        let extension = path.extension().unwrap_or_default().to_str().unwrap();
                        new_destination =
                            format!("{}/{}_{}.{}", parent, file_stem, counter, extension);
                        counter += 1;
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }
        match std::fs::rename(source, new_destination) {
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
        reset_terminal();
    }
}

/// * `main` - Entry point of the program.
#[cfg(not(tarpaulin_include))]
fn main() {
    // Record the start time
    let start = Instant::now();

    let file_ops = RealFileOperations;

    // Create an instance of TerminalGuard that will be dropped when main exits
    let _guard = TerminalGuard;

    // we need to test if the command line args passed in were valid
    // if they aren't then have print the error and exit
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(e) => {
            println!("{}", e);
            println!();
            std::process::exit(-1);
        }
    };

    setup_terminal();

    print_banner();

    if get_command_line_arguments(&args).is_err() {
        reset_terminal();
        std::process::exit(-1);
    }

    //setup_ctrlc_handler();

    match start_search(&file_ops, &args) {
        Ok(search_results) => {
            let duration = start.elapsed();
            println!("Elapsed time: {}", humantime::format_duration(duration));
            if search_results.number_duplicates == 0 {
                println!("No duplicates found");
            } else {
                println!(
                    "Found {} set of duplicates with total size {}",
                    search_results.number_duplicates,
                    bytesize::ByteSize(search_results.total_size.try_into().unwrap())
                );
                println!();
                println!();
            }
            reset_terminal();
            std::process::exit(search_results.number_duplicates.try_into().unwrap());
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            reset_terminal();
            std::process::exit(-1)
        }
    }
}

/// # print_banner
/// Function to print the banner to the terminal.
fn print_banner() {
    println!("{}", style("dupefindr").bold());
}

/// # setup_terminal
/// Setup the terminal for the program.
fn setup_terminal() {
    //let _ = terminal::enable_raw_mode();

    // Clear the screen
    // let _ = execute!(
    //     stdout(),
    //     style::ResetColor,
    //     terminal::Clear(ClearType::All),
    //     cursor::Hide,
    //     cursor::MoveTo(0, 0)
    // );
}

/// # reset_terminal
/// Reset the terminal.
fn reset_terminal() {
    // io::stdout().flush().unwrap();
    // let _ = execute!(stdout(), style::ResetColor, cursor::Show,);
    //let _ = terminal::disable_raw_mode();
}

/// # get_command_line_arguments
/// Gets the command line arguments object.  Not included in testing since there are no command lines passed in
#[cfg(not(tarpaulin_include))]
fn get_command_line_arguments(args: &Args) -> Result<(), std::io::Error> {
    if args.shared.debug {
        let default_parallelism_approx = num_cpus::get();
        println!("Command: {:?}", args.command);
        println!("Searching for duplicates in: {}", args.shared.path);
        println!(
            "Recursively searching for duplicates: {}",
            args.shared.recursive
        );
        println!("Include empty files: {}", args.shared.include_empty_files);
        println!("Dry run: {}", args.shared.dry_run);
        println!("Include hidden files: {}", args.shared.include_hidden_files);
        println!("Verbose: {}", args.shared.verbose);
        println!("Quiet: {}", args.shared.quiet);
        println!("Wildcard: {}", args.shared.wildcard);
        println!("Exclusion wildcard: {}", args.shared.exclusion_wildcard);
        println!("Available cpus: {}", default_parallelism_approx);
        println!("Create Report: {}", args.shared.create_report);
        println!("Report Path: {}", args.shared.report_path);
        println!();
    }

    // validate
    // if create report is true, then validate the report_path
    // rather do it now, that later
    if args.shared.create_report {
        // attempt to create a file specified by report_path
        if let Err(e) = std::fs::File::create(&args.shared.report_path) {
            eprintln!("Invalid report file path: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// # get_number_of_threads
/// Get the number of threads to use for thread pools
/// * `args` - The command line arguments.
/// * `usize` - The number of threads to use.
fn get_number_of_threads(args: &Args) -> usize {
    let default_parallelism_approx = num_cpus::get();
    let max_threads = args.shared.max_threads.unwrap_or(0);
    if max_threads == 0 {
        default_parallelism_approx
    } else {
        max_threads
    }
}

/// # start_search
/// Start the search for duplicate files.
/// * `file_ops` - The file operations object.
/// * `args` - The command line arguments.
/// * `running` - The running flag.
/// * `Result<SearchResults, io::Error>` - The search results.
/// # Errors
/// * `io::Error` - An error occurred during the search.
fn start_search<T: FileOperations>(file_ops: &T, args: &Args) -> Result<SearchResults, io::Error> {
    // get the files in the directory
    let folder_path: String = args.shared.path.clone();

    // get the files in the directory
    // it calls itself as it traverses the tree if recursive is set
    let multi = MultiProgress::new();
    let result = get_files_in_directory(args, folder_path, &multi, true);
    let files = match result {
        Ok(files) => files,
        Err(e) => {
            println!("Error: {}", e);
            return Err(e);
        }
    };
    if args.shared.verbose {
        println!("Found {} files", files.len());
    }

    // identify the duplicates
    let full_hash_map = identify_duplicates(args, files);
    // process the duplicates
    let dup_fileset_vec = process_duplicates(file_ops, args, &full_hash_map);

    // print the duplicate results
    let duplicates_found = dup_fileset_vec.len();
    let mut duplicates_total_size: i64 = 0;
    for dup_fileset in dup_fileset_vec.iter() {
        if args.shared.verbose {
            println!(
                "Found {} duplicates for hash: {}",
                dup_fileset.extras.len(),
                dup_fileset.hash
            );
        }
        for file in &dup_fileset.extras {
            if args.shared.verbose {
                println!(
                    "File: {} [created: {}] [modified: {}] [{} bytes]",
                    file.path,
                    file.created_at.to_rfc2822(),
                    file.modified_at.to_rfc2822(),
                    bytesize::ByteSize(file.size)
                );
            }
            duplicates_total_size += file.size as i64;
            if args.shared.verbose {
                println!();
            }
        }
    }

    // create report if configured
    if args.shared.create_report {
        let _ = create_duplicate_report(args, dup_fileset_vec);
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
    multi: &MultiProgress,
    first_run: bool,
) -> Result<Vec<FileInfo>, io::Error> {
    let mut files: Vec<FileInfo> = Vec::new();

    // check if the path is a directory
    match fs::metadata(folder_path.as_str()) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                eprintln!("The path provided {} is not a directory", folder_path);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "The path provided is not a directory",
                ));
            }
        }
        Err(e) => {
            eprintln!("Error calling fs::metadata with path {}", folder_path);
            return Err(e);
        }
    }
    if args.shared.debug {
        let _ = multi.println(format!("Collecting objects in: {}", folder_path));
    }

    // collect the entries in the directory
    let entries = fs::read_dir(&folder_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
    if args.shared.debug {
        let _ = multi.println(format!("Finished collecting objects in: {}", folder_path));
    }

    // only add a spinner if the multi is empty
    let bar = if args.shared.quiet {
        multi.add(ProgressBar::hidden())
    } else {
        // only add the spinner if this is the top level
        if first_run {
            let b = multi.add(ProgressBar::new_spinner().with_message("Processing files..."));
            b.enable_steady_tick(Duration::from_millis(100));
            b.set_style(ProgressStyle::with_template("{spinner:.blue} {msg}").unwrap());
            b
        } else {
            multi.add(ProgressBar::hidden())
        }
    };

    let mut folder_count = 0;
    let mut file_count = 0;
    let mut folders: Vec<PathBuf> = Vec::new();
    let workers = get_number_of_threads(args);
    let pool = ThreadPool::new(workers);
    let (tx, rx) = channel();
    let files_count = entries.len();

    if args.shared.debug {
        let _ = multi.println(format!("Iterating entries: {}", folder_path));
    }

    // use thread pool to optimize the process of scanning then directory objects
    // if there are a lot of folders and/or files in the directory, this will speed up the process
    for entry in entries.iter() {
        let tx = tx.clone();
        let entry = entry.clone();

        pool.execute(move || {
            // check if the entry is a directory
            let is_dir = entry.is_dir();
            tx.send((entry, is_dir)).unwrap_or_default();
        });
    }
    if args.shared.debug {
        let _ = multi.println(format!("Completed iterating entries: {}", folder_path));
    }

    // wait for the jobs to complete, and process the results
    let mut processed = 0;
    while processed < files_count {
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

    // process the folders
    if folder_count != 0 {
        let bar2 = if args.shared.quiet {
            multi.add(ProgressBar::hidden())
        } else {
            let b = multi.add(ProgressBar::new(folder_count));
            b.set_style(
                ProgressStyle::with_template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}").unwrap(), //.progress_chars("##-"),
            );
            b
        };

        for fld in folders.iter() {
            bar2.set_message(format!("Folder {}", fld.display()));
            let hidden;
            // check if the folder is hidden - use appropriate code for the OS
            #[cfg(not(target_os = "windows"))]
            {
                hidden = fld.file_name().unwrap().to_str().unwrap().starts_with(".");
            }
            #[cfg(target_os = "windows")]
            {
                let md = std::fs::metadata(fld);
                let fa = md.unwrap().file_attributes();
                if fa & 0x00000002 != 0 {
                    hidden = true;
                } else {
                    hidden = false;
                }
            }

            if hidden && !args.shared.include_hidden_files {
                if args.shared.verbose {
                    let _ = multi.println(format!(
                        "Ignoring hidden directory: {}",
                        fld.file_name().unwrap().to_str().unwrap()
                    ));
                }
                bar2.inc(1);
                continue;
            }

            // if we aren't recursive, then ignore any folders we find
            if !args.shared.recursive {
                if args.shared.verbose {
                    let _ = multi.println(format!(
                        "Ignoring directory: {}",
                        fld.file_name().unwrap().to_str().unwrap()
                    ));
                }
                bar2.inc(1);
            } else {
                // if we are recursive, then process the sub folders
                let path = fld.as_path();
                // recursion call
                let sub_files =
                    get_files_in_directory(args, path.to_str().unwrap().to_string(), multi, false)?;
                // add results to our files vector
                files.extend(sub_files);
                bar2.inc(1);
            }
        }

        // remove the progress bar for this folder
        bar2.finish_and_clear();
        multi.remove(&bar2);
    }

    // now process files
    if file_count != 0 {
        let bar2 = if args.shared.quiet {
            multi.add(ProgressBar::hidden())
        } else {
            multi.add(ProgressBar::new(file_count))
        };

        if !bar2.is_hidden() {
            bar2.set_style(
                ProgressStyle::with_template("{bar:40.green/yellow} {pos:>7}/{len:7} {msg}")
                    .unwrap()
                    .progress_chars("##-"),
            );
        }

        for entry in entries.iter() {
            let path = entry.as_path();
            bar2.set_message(format!("Processing: {}", path.display()));

            if path.is_file() {
                // determine if the file matches the wildcard
                let wildcard_pattern = glob::Pattern::new(&args.shared.wildcard)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                if !wildcard_pattern.matches_path(path) {
                    if args.shared.verbose {
                        let _ = multi.println(format!(
                            "Ignoring file (does not match wildcard): {}",
                            path.to_str().unwrap()
                        ));
                    }
                    bar2.inc(1);
                    continue;
                }
                // determine if the file matches the exclusion wildcard
                if !args.shared.exclusion_wildcard.is_empty() {
                    let exclusion_wildcard_pattern =
                        glob::Pattern::new(&args.shared.exclusion_wildcard)
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                    if exclusion_wildcard_pattern.matches_path(path) {
                        if args.shared.verbose {
                            let _ = multi.println(format!(
                                "Ignoring file (matches exclusion wildcard): {}",
                                path.to_str().unwrap()
                            ));
                        }
                        bar2.inc(1);
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
                    if std::fs::metadata(&path).unwrap().file_attributes() & 0x00000002 != 0 {
                        hidden = true;
                    } else {
                        hidden = false;
                    }
                }
                if !args.shared.include_hidden_files && hidden {
                    // skip hidden files if not including them
                    if args.shared.verbose {
                        let _ = multi
                            .println(format!("Ignoring hidden file: {}", path.to_str().unwrap()));
                    }

                    bar2.inc(1);
                    continue;
                }
                // get the file metadata
                let meta = std::fs::metadata(path).unwrap();
                let size = meta.len();
                if size == 0 && !args.shared.include_empty_files {
                    // skip empty files if not including them
                    if args.shared.verbose {
                        let _ = multi
                            .println(format!("Ignoring empty file: {}", path.to_str().unwrap()));
                    }

                    bar2.inc(1);
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
                    let _ = multi.println(format!(
                        "Selected File: {} [created: {}] [modified: {}] [{} bytes]",
                        path.to_str().unwrap(),
                        created_at_utc_datetime.to_rfc2822(),
                        modified_at_utc_datetime.to_rfc2822(),
                        size
                    ));
                }
                bar2.inc(1);
            }
        }

        bar2.finish();
        multi.remove(&bar2);
    }

    bar.finish_and_clear();
    multi.remove(&bar);
    Ok(files)
}

/// # identify_duplicates
/// Identify duplicate files based on their MD5 hash
/// * `args` - The command line arguments.
/// * `files` - The files to process.
/// * `running` - The running flag.
fn identify_duplicates(args: &Args, files: Vec<FileInfo>) -> HashMap<String, Vec<FileInfo>> {
    let mut hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
    let multi = MultiProgress::new();
    let workers = get_number_of_threads(args);

    let bar2 = if args.shared.quiet {
        multi.add(ProgressBar::hidden())
    } else {
        multi.add(ProgressBar::new_spinner().with_message("Identifying duplicates..."))
    };

    bar2.enable_steady_tick(Duration::from_millis(100));

    let bar = if args.shared.quiet {
        multi.add(ProgressBar::hidden())
    } else {
        multi.add(ProgressBar::new(files.len().try_into().unwrap()))
    };

    bar.set_style(
        ProgressStyle::with_template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

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
                    eprintln!("{}", e);
                    tx.send((String::new(), file.clone())).unwrap()
                }
            }
        });
    }

    // wait for the jobs to complete, and process the results
    rx.iter().take(files_count).for_each(|(hash_string, file)| {
        if hash_string.is_empty() {
            if args.shared.debug {
                let _ = multi.println(format!(
                    "File: {} [{} bytes] [error calculating hash]",
                    file.path, file.size
                ));
            }
            return;
        }
        if args.shared.verbose {
            let _ = multi.println(format!(
                "File: {} [{} bytes] [hash: {}]",
                file.path, file.size, hash_string
            ));
        }
        // add the file and hash to the map
        // if the hash doesn't exist, create a new vector
        if !hash_map.contains_key(&hash_string) {
            let vec = vec![file];
            hash_map.insert(hash_string.to_string(), vec);
        } else {
            let vec = hash_map.get_mut(&hash_string).unwrap();
            vec.push(file);
        }
        bar.inc(1);
    });

    bar.finish();
    bar2.finish();

    multi.remove(&bar2);
    multi.remove(&bar);
    multi.clear().unwrap();

    hash_map
}

/// # process_duplicates
/// Process the duplicate files using the method specified in cmd line args
/// * `file_ops` - The file operations object.
/// * `args` - The command line arguments.
/// * `hash_map` - The hash map of files.
/// * `running` - The running flag.
/// # Returns
/// An Array of DuplicateFileSet
fn process_duplicates<T: FileOperations>(
    file_ops: &T,
    args: &Args,
    hash_map: &HashMap<String, Vec<FileInfo>>,
) -> Vec<DuplicateFileSet> {
    let mut new_hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();

    let mut multi = MultiProgress::new();

    let bar2 = if args.shared.quiet {
        multi.add(ProgressBar::hidden())
    } else {
        multi.add(ProgressBar::new_spinner().with_message("Processing duplicates..."))
    };

    bar2.enable_steady_tick(Duration::from_millis(100));

    let bar = if args.shared.quiet {
        multi.add(ProgressBar::hidden())
    } else {
        multi.add(ProgressBar::new(hash_map.len().try_into().unwrap()))
    };

    bar.set_style(
        ProgressStyle::with_template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    // get the method
    let method = match &args.command {
        Commands::Move { method, .. } => method,
        Commands::Copy { method, .. } => method,
        Commands::Delete { method } => method,
        Commands::Find { method } => method,
    };

    // if the duplicate selection method is "interactive" then we need to turn off the progress bars
    if *method == DuplicateSelectionMethod::Interactive {
        bar.finish();
        bar2.finish();
        multi.remove(&bar2);
        multi.remove(&bar);
        multi.clear().unwrap();
    }

    // remove all entries from hash_map where the files.len() <= 1
    let mut hash_map = hash_map.clone();
    hash_map.retain(|_, files| files.len() > 1);

    // store the results
    let mut dup_results: Vec<DuplicateFileSet> = Vec::new();

    // get list of files to process
    for (index, (hash, files)) in hash_map.iter().enumerate() {
        new_hash_map.insert(hash.clone(), files.clone());

        // if the command is FindDuplicates, then we don't need to process the duplicates
        if let Commands::Find { .. } = args.command {
            continue;
        }

        let dup_fileset = match select_duplicate_files(
            args.command.clone(),
            method.clone(),
            hash,
            files,
            index + 1,
            hash_map.len(),
            &bar2,
        ) {
            Ok(dup_fileset) => dup_fileset,
            Err(e) => {
                if e.kind() == InteractiveErrorKind::Skip {
                    DuplicateFileSet {
                        hash: hash.to_string(),
                        keeper: None,
                        extras: vec![],
                        result: DuplicateResult::Skipped,
                    }
                } else {
                    DuplicateFileSet {
                        hash: hash.to_string(),
                        keeper: None,
                        extras: vec![],
                        result: DuplicateResult::Aborted,
                    }
                }
            }
        };
        if dup_fileset.result == DuplicateResult::Aborted {
            break;
        }
        // only process if there is a file to process
        if dup_fileset.keeper.is_some() {
            if args.shared.debug {
                if let Some(ref keeper) = dup_fileset.keeper {
                    let _ = multi.println(format!("Selected File: {}", keeper.path));
                }
            }

            for file in &dup_fileset.extras {
                let _ = process_a_duplicate_file(file_ops, args, file, hash, &mut multi);
                yield_now();
            }
        }

        dup_results.push(dup_fileset);

        bar.inc(1);
    }

    bar.finish();
    bar2.finish();
    multi.remove(&bar2);
    multi.remove(&bar);
    multi.clear().unwrap();
    dup_results
}

/// # process_a_duplicate_file
/// Process a duplicate file based on the command line arguments
/// * `file_ops` - The file operations object.
/// * `args` - The command line arguments.
/// * `file` - The file to process.
/// * `hash` - The hash of the file.
/// * `multi` - The progress bar.
/// * `Result<(), std::io::Error>` - The result of the operation.
/// # Errors
/// * `std::io::Error` - An error occurred during the operation.
fn process_a_duplicate_file<T: FileOperations>(
    file_ops: &T,
    args: &Args,
    file: &FileInfo,
    hash: &str,
    multi: &mut MultiProgress,
) -> Result<(), std::io::Error> {
    let source = &file.path;
    //let file_name = Path::new(&file.path).file_name().unwrap().to_str().unwrap();
    let location = match &args.command {
        Commands::Move { location, .. } => location,
        Commands::Copy { location, .. } => location,
        Commands::Delete { method: _ } => "",
        Commands::Find { method: _ } => "",
    };

    let flatten = match &args.command {
        Commands::Move { flatten, .. } => *flatten,
        Commands::Copy { flatten, .. } => *flatten,
        Commands::Delete { method: _ } => false,
        Commands::Find { method: _ } => false,
    };

    let no_hash_folder = match &args.command {
        Commands::Move { no_hash_folder, .. } => *no_hash_folder,
        Commands::Copy { no_hash_folder, .. } => *no_hash_folder,
        Commands::Delete { method: _ } => false,
        Commands::Find { method: _ } => false,
    };

    let overwrite = match &args.command {
        Commands::Move { overwrite, .. } => *overwrite,
        Commands::Copy { overwrite, .. } => *overwrite,
        Commands::Delete { method: _ } => false,
        Commands::Find { method: _ } => false,
    };

    let relative_path = Path::new(&file.path)
        .strip_prefix(&args.shared.path)
        .unwrap_or_else(|_| Path::new(&file.path))
        .to_str()
        .unwrap()
        .to_string();

    let mut destination_folder;

    if !flatten {
        // destination folder is the relative path of the file with the hash appended
        destination_folder = Path::new(location)
            .join(&relative_path)
            .parent()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
    } else {
        destination_folder = location.to_string();
    }
    if !no_hash_folder {
        #[cfg(target_os = "windows")]
        destination_folder.push_str("\\");
        #[cfg(not(target_os = "windows"))]
        destination_folder.push('/');
        destination_folder.push_str(hash);
    }

    let mut destination = destination_folder.clone();

    #[cfg(target_os = "windows")]
    destination.push_str("\\");
    #[cfg(not(target_os = "windows"))]
    destination.push('/');
    destination.push_str(Path::new(&file.path).file_name().unwrap().to_str().unwrap());

    let mut error: Option<std::io::Error> = None;

    let command_text: String = match args.command {
        Commands::Find { .. } => "Find".to_string(),
        Commands::Move { .. } => "Move".to_string(),
        Commands::Copy { .. } => "Copy".to_string(),
        Commands::Delete { .. } => "Delete".to_string(),
    };

    // if not a dry run, then perform the operation
    if !args.shared.dry_run {
        if args.shared.verbose {
            // location is empty for Find and Delete commands
            if location.is_empty() {
                let _ = multi.println(format!("{}ing: {}", command_text, source));
            } else {
                let _ = multi.println(format!(
                    "{}ing: {} to {}",
                    command_text, source, destination
                ));
            }
        }

        match args.command {
            Commands::Find { .. } => {}
            Commands::Move { .. } => {
                if let Err(result) = file_ops.rename(source, &destination, overwrite) {
                    error = Some(result);
                }
            }
            Commands::Copy { .. } => {
                if let Err(result) = fs::create_dir_all(&destination_folder) {
                    let _ = multi.println(
                        format!(
                            "Error creating directory: {} - {}",
                            destination_folder, result
                        )
                        .as_str(),
                    );
                }
                if let Err(result) = file_ops.copy(source, &destination, overwrite) {
                    error = Some(result);
                }
            }
            Commands::Delete { .. } => {
                if let Err(result) = file_ops.remove_file(source) {
                    error = Some(result);
                }
            }
        }

        if error.is_some() {
            let _ = multi.println(format!(
                "*** Failed to {} {} to {}: {:?}",
                command_text, source, destination, error
            ));
        }
    } else if args.shared.verbose {
        let _ = multi.println(format!(
            "Dry run: Would {} {} to {}",
            command_text, source, destination
        ));
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
fn get_hash_of_file(file_path: &str, _bar: &ProgressBar) -> Result<String, std::io::Error> {
    let result = std::fs::File::open(file_path);
    match result {
        Ok(mut f) => {
            //let mut file = std::fs::File::open(file_path).unwrap();
            let mut hasher = md5::Md5::new();
            let mut buffer = [0; BUFFER_READ_SIZE]; // Read in chunks

            loop {
                let bytes_read = f.read(&mut buffer)?;
                if bytes_read == 0 {
                    break;
                }
                // Normalize line endings by replacing \r\n with \n
                let normalized_buffer: Vec<u8> = buffer[..bytes_read]
                    .iter()
                    .flat_map(|&b| if b == b'\r' { None } else { Some(b) })
                    .collect();
                hasher.update(&normalized_buffer);
            }

            let hash = hasher.finalize();
            Ok(format!("{:x}", hash))
        }
        Err(e) => {
            eprintln!("{:?}", e);
            Err(e)
        }
    }
}

/// # select_duplicate_files
/// Select the duplicate files based on the method specified in the command line arguments
/// * `command` - the command used (Find,Copy,Move,Delete)
/// * `method` - The method to use.
/// * `hash` - The hash of the files.
/// * `files` - The files to process.
/// * `position_duplicates` - The index in the list of duplictes
/// * `total_duplicates` - The total number of duplicates
/// * `bar` - The progress bar.
/// # Returns
/// * `DuplicateFileSet` - The set of duplicate files.
/// # `Error` - An Error or the user pressed ESC
fn select_duplicate_files(
    command: Commands,
    method: DuplicateSelectionMethod,
    hash: &String,
    files: &[FileInfo],
    position_duplicates: usize,
    total_duplicates: usize,
    _bar: &ProgressBar,
) -> Result<DuplicateFileSet, InteractiveError> {
    let mut dup_fileset = DuplicateFileSet {
        hash: hash.to_string(),
        keeper: None,
        extras: vec![],
        result: DuplicateResult::Aborted,
    };
    if files.is_empty() {
        return Ok(dup_fileset);
    }
    match command {
        Commands::Find { .. } => dup_fileset.result = DuplicateResult::Found,
        Commands::Move { .. } => dup_fileset.result = DuplicateResult::Moved,
        Commands::Copy { .. } => dup_fileset.result = DuplicateResult::Copied,
        Commands::Delete { .. } => dup_fileset.result = DuplicateResult::Deleted,
    }
    match method {
        DuplicateSelectionMethod::Newest => {
            // keep the newest file, so return all other files
            let mut sorted_files = files.to_owned();
            sorted_files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
            let keeper = sorted_files.swap_remove(0);
            dup_fileset.keeper = Some(keeper);
            dup_fileset.extras = sorted_files;
        }
        DuplicateSelectionMethod::Oldest => {
            // keep the oldest file, so return all other files
            let mut sorted_files = files.to_owned();
            sorted_files.sort_by(|a, b| a.modified_at.cmp(&b.modified_at));
            let keeper = sorted_files.swap_remove(0);
            dup_fileset.keeper = Some(keeper);
            dup_fileset.extras = sorted_files;
        }
        // not sure how to test the interactive code right now
        #[cfg(not(tarpaulin_include))]
        DuplicateSelectionMethod::Interactive => {
            dup_fileset.extras = files.to_owned();
            let title = format!(
                "Duplicate File Interactive Selector [{}/{}]",
                position_duplicates, total_duplicates
            );
            println!();
            println!("{}", style(title).bold());
            println!();
            println!("Use ARROW keys to select a file to keep");
            println!("Press ENTER to keep the selected file and process the rest");
            println!("Press S to skip to the next duplicate");
            println!("Press ESC to exit the program");
            println!();
            println!("For hash [{}]:", hash);
            println!();

            dup_fileset.keeper = get_interactive_selection(files)?
        }
    }

    Ok(dup_fileset)
}

fn get_interactive_selection(files: &[FileInfo]) -> Result<Option<FileInfo>, InteractiveError> {
    // convert files into a string array
    let file_strings: Vec<String> = files
        .iter()
        .map(|file| {
            //{:<50} {:<20} {:<20}
            format!(
                "{:<50} [c:{:<20}] [m:{:<20}]",
                file.path,
                file.created_at
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S"),
                file.modified_at
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
            )
        })
        .collect();

    let keys = vec![Key::Char('s')];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select file to keep:")
        .items(&file_strings)
        .max_length(5)
        .interact_opt_with_keys(&keys)
        .unwrap();

    // if selection.key is not none, then check to see what key the user pressed
    if selection.key.is_some() {
        let key = selection.key.unwrap();
        if key == Key::Char('s') {
            Err(InteractiveError::Skip())
        } else {
            Err(InteractiveError::Other(format!("{:?}", key)))
        }
    } else if selection.index.is_none() {
        // user press escape
        Err(InteractiveError::Escape())
    } else {
        let index = selection.index.unwrap();
        Ok(Some(files[index].clone()))
    }
}

fn create_duplicate_report(
    args: &Args,
    dup_fileset_vec: Vec<DuplicateFileSet>,
) -> Result<(), std::io::Error> {
    if !args.shared.create_report {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Report creation is disabled",
        ));
    }

    let mut wtr = csv::Writer::from_path(&args.shared.report_path)?;

    wtr.write_record([
        "Hash",
        "File Path",
        "Size",
        "Created At",
        "Modified At",
        "Result",
    ])?;

    for dup_fileset in dup_fileset_vec.iter() {
        for file in &dup_fileset.extras {
            wtr.write_record(&[
                dup_fileset.hash.clone(),
                file.path.clone(),
                file.size.to_string(),
                file.created_at.to_rfc3339(),
                file.modified_at.to_rfc3339(),
                format!("{:?}", dup_fileset.result),
            ])?;
        }
    }

    wtr.flush()?;
    Ok(())
}

/// # Tests
///
/// Unit tests for the various functions and features of the program.
#[cfg(test)]
mod tests {

    use super::*;
    use csv::ReaderBuilder;
    use tempfile::tempdir;

    // setup mock file operations

    struct MockFileOperationsOk;

    impl FileOperations for MockFileOperationsOk {
        fn copy(
            &self,
            _source: &str,
            _destination: &str,
            _overwrite: bool,
        ) -> Result<(), std::io::Error> {
            // Mock implementation
            Ok(())
        }

        fn remove_file(&self, _source: &str) -> Result<(), std::io::Error> {
            // Mock implementation
            Ok(())
        }

        fn rename(
            &self,
            _source: &str,
            _destination: &str,
            _overwrite: bool,
        ) -> Result<(), std::io::Error> {
            // Mock implementation
            Ok(())
        }
    }

    struct MockFileOperationsError;

    impl FileOperations for MockFileOperationsError {
        fn copy(
            &self,
            _source: &str,
            _destination: &str,
            _overwrite: bool,
        ) -> Result<(), std::io::Error> {
            // Mock implementation - produce an error
            Err(io::Error::new(io::ErrorKind::Other, "Mock error"))
        }

        fn remove_file(&self, _source: &str) -> Result<(), std::io::Error> {
            // Mock implementation - produce an error
            Err(io::Error::new(io::ErrorKind::Other, "Mock error"))
        }

        fn rename(
            &self,
            _source: &str,
            _destination: &str,
            _overwrite: bool,
        ) -> Result<(), std::io::Error> {
            // Mock implementation - produce an error
            Err(io::Error::new(io::ErrorKind::Other, "Mock error"))
        }
    }

    fn create_default_command_line_arguments() -> Args {
        let shared_options = SharedOptions {
            path: "testdata".to_string(),
            recursive: false,
            debug: false,
            include_empty_files: false,
            dry_run: true,
            include_hidden_files: false,
            verbose: true,
            quiet: false,
            wildcard: "*".to_string(),
            exclusion_wildcard: "".to_string(),
            max_threads: Some(0),
            create_report: false,
            report_path: "./dupefinder-report.csv".to_string(),
        };
        let s1 = shared_options.clone();
        Args {
            shared: s1,
            command: Commands::Find {
                method: DuplicateSelectionMethod::Newest,
            },
        }
    }

    #[test]
    fn test_get_command_line_args() {
        let args = create_default_command_line_arguments();
        assert!(get_command_line_arguments(&args).is_ok());
    }

    #[test]
    fn test_get_command_line_args_bad_report_path() {
        let mut args = create_default_command_line_arguments();
        args.shared.create_report = true;
        args.shared.report_path = "/this/is/a/path/with/invalid/characters/\0/and/also/too/long/on/windows////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////0".to_string();
        assert!(get_command_line_arguments(&args).is_err());
    }

    #[test]
    fn test_get_files_in_directory() {
        let args = create_default_command_line_arguments();
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        // under windows .testhidden is not considered a hidden file
        #[cfg(target_os = "windows")]
        assert_eq!(files.len(), 9);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(files.len(), 8);
    }

    #[test]
    fn test_get_files_in_directory_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.quiet = true;
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        // under windows .testhidden is not considered a hidden file
        #[cfg(target_os = "windows")]
        assert_eq!(files.len(), 9);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(files.len(), 8);
    }

    #[test]
    fn test_get_files_in_directory_wildcard() {
        let mut args = create_default_command_line_arguments();
        args.shared.wildcard = "*testdupe*.txt".to_string();
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        assert_eq!(files.len(), 7);
    }

    #[test]
    fn test_get_files_in_directory_exclusion_wildcard() {
        let mut args = create_default_command_line_arguments();
        args.shared.exclusion_wildcard = "*testdupe*.txt".to_string();
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        // under windows .testhidden is not considered a hidden file
        #[cfg(target_os = "windows")]
        assert_eq!(files.len(), 2);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_get_files_in_directory_include_empty() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_empty_files = true;
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        #[cfg(target_os = "windows")]
        assert_eq!(files.len(), 11);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(files.len(), 10);
    }

    #[test]
    fn test_get_files_in_directory_include_hidden() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_hidden_files = true;
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        assert_eq!(files.len(), 9);
    }

    #[test]
    fn test_get_files_in_directory_include_all_files() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_hidden_files = true;
        args.shared.include_empty_files = true;
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        assert_eq!(files.len(), 11);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        #[cfg(target_os = "windows")]
        assert_eq!(files.len(), 22);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(files.len(), 19);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive_with_hidden() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        args.shared.include_hidden_files = true;
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        assert_eq!(files.len(), 22);
    }

    #[test]
    fn test_get_files_in_directory_bad_path() {
        let mut args = create_default_command_line_arguments();
        args.shared.path = "badpath!!!".to_string();
        let multi = MultiProgress::new();
        let result = get_files_in_directory(&args, "badpath!!!".to_string(), &multi, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_files_in_directory_notafolder() {
        let args = create_default_command_line_arguments();
        let multi = MultiProgress::new();
        let result = get_files_in_directory(
            &args,
            format!("{}/testnodupe.txt", args.shared.path),
            &multi,
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_get_hash_of_file() {
        let args = create_default_command_line_arguments();
        let hash = get_hash_of_file(
            &format!("{}//testdupe1.txt", args.shared.path.clone()),
            &ProgressBar::new_spinner().with_message("none"),
        );
        assert!(hash.is_ok());
        assert_eq!(hash.unwrap(), "8c91214730e59f67bd46d1855156e762");
        //assert_eq!(hash.unwrap(), "710c2d261165da2eac0e2321ea9ddbed");
    }

    #[test]
    fn test_get_hash_of_file_bad_path() {
        let args = create_default_command_line_arguments();
        let hash = get_hash_of_file(
            &format!("{}//testdupe1-notfound.txt", args.shared.path.clone()),
            &ProgressBar::new_spinner().with_message("none"),
        );
        assert!(hash.is_err());
    }

    #[test]
    fn test_get_number_of_threads_with_max_threads_0() {
        let args = create_default_command_line_arguments();
        let threads = get_number_of_threads(&args);
        let num_cpus = num_cpus::get();
        assert_eq!(threads, num_cpus);
    }

    #[test]
    fn test_get_number_of_threads_with_max_threads_2() {
        let mut args = create_default_command_line_arguments();
        args.shared.max_threads = Some(2);
        let threads = get_number_of_threads(&args);
        assert_eq!(threads, 2);
    }

    #[test]
    fn test_start_search() {
        let args = create_default_command_line_arguments();
        let file_ops = RealFileOperations;

        let result = start_search(&file_ops, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_copy() {
        let mut args = create_default_command_line_arguments();
        args.command = Commands::Copy {
            location: "/tmp".to_string(),
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: true,
        };
        let file_ops = MockFileOperationsOk;

        let result = start_search(&file_ops, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_move() {
        let mut args = create_default_command_line_arguments();
        args.command = Commands::Move {
            location: "/tmp".to_string(),
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: true,
        };
        let file_ops = MockFileOperationsOk;

        let result = start_search(&file_ops, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_delete() {
        let mut args = create_default_command_line_arguments();
        args.command = Commands::Delete {
            method: DuplicateSelectionMethod::Newest,
        };
        let file_ops = MockFileOperationsOk;

        let result = start_search(&file_ops, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.quiet = true;
        let file_ops = RealFileOperations;

        let result = start_search(&file_ops, &args);
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

        let file_ops = RealFileOperations;
        let result = start_search(&file_ops, &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_start_search_copy_realfileops() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        args.shared.dry_run = true;
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap().to_string();

        println!("Temporary location : {}", temp_path);

        args.command = Commands::Copy {
            location: temp_path,
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };
        let file_ops = RealFileOperations;
        let result = start_search(&file_ops, &args);

        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_bad_path() {
        let mut args = create_default_command_line_arguments();
        args.shared.path = "data-badpath!!!".to_string();
        let file_ops = RealFileOperations;

        let result = start_search(&file_ops, &args);
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

        args.command = Commands::Copy {
            location: temp_path,
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };
        let file_ops = RealFileOperations;
        let result = start_search(&file_ops, &args);

        assert!(result.is_ok());
    }

    #[test]
    fn test_create_report_arg_is_false() {
        let mut args = create_default_command_line_arguments();
        args.shared.create_report = false;
        let dup_fileset_vec = Vec::new();
        assert!(create_duplicate_report(&args, dup_fileset_vec).is_err());
    }

    #[test]
    fn test_create_report() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        args.shared.dry_run = true;
        args.shared.wildcard = "testnodupe.txt".to_owned();
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap().to_string();
        args.shared.create_report = true;
        args.shared.report_path = "./testreport.csv".to_string();

        args.command = Commands::Copy {
            location: temp_path,
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };

        let file_ops = RealFileOperations;
        let result = start_search(&file_ops, &args);

        assert!(result.is_ok());
        // test to see if report file was created
        assert!(std::path::Path::new("./testreport.csv").exists());
        // test to see if report file is valid csv
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .from_path("./testreport.csv")
            .unwrap();
        assert!(rdr.headers().is_ok());
        // cleanup
        std::fs::remove_file("./testreport.csv").unwrap();
    }

    #[test]
    fn test_identify_duplicates() {
        let args = create_default_command_line_arguments();
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        let hash_map = identify_duplicates(&args, files);
        // duplicates are entries in hash_map with more than 1 file
        let mut duplicates_found = 0;
        for (_hash, files) in hash_map.iter() {
            if files.len() > 1 {
                duplicates_found += 1;
            }
        }
        assert_eq!(duplicates_found, 2);
    }

    #[test]
    fn test_identify_duplicates_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.quiet = true;
        let multi = MultiProgress::new();
        let files = get_files_in_directory(&args, args.shared.path.clone(), &multi, true).unwrap();
        let hash_map = identify_duplicates(&args, files);
        // duplicates are entries in hash_map with more than 1 file
        let mut duplicates_found = 0;
        for (_hash, files) in hash_map.iter() {
            if files.len() > 1 {
                duplicates_found += 1;
            }
        }
        assert_eq!(duplicates_found, 2);
    }

    #[test]
    fn test_identify_duplicates_no_files() {
        let args = create_default_command_line_arguments();

        let files = Vec::new();
        let hash_map = identify_duplicates(&args, files);
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

        let mut files = Vec::new();
        let file = FileInfo {
            path: "todo!()".to_owned(),
            size: 123,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        files.push(file);
        let hash_map = identify_duplicates(&args, files);
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
        let bar = ProgressBar::new_spinner().with_message("none");
        let dup_fileset = select_duplicate_files(
            args.command.clone(),
            DuplicateSelectionMethod::Newest,
            &"testhash".to_owned(),
            &files,
            1,
            1,
            &bar,
        )
        .unwrap();
        assert!(dup_fileset.keeper.is_some());
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
        let bar = ProgressBar::new_spinner().with_message("none");
        let dup_fileset = select_duplicate_files(
            args.command.clone(),
            DuplicateSelectionMethod::Oldest,
            &"testhash".to_owned(),
            &files,
            1,
            1,
            &bar,
        )
        .unwrap();
        assert!(dup_fileset.keeper.is_some());
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
        let args = create_default_command_line_arguments();
        let files = Vec::new();
        let bar = ProgressBar::new_spinner().with_message("none");
        let dup_fileset = select_duplicate_files(
            args.command.clone(),
            DuplicateSelectionMethod::Oldest,
            &"testhash".to_owned(),
            &files,
            1,
            1,
            &bar,
        )
        .unwrap();
        assert!(dup_fileset.keeper.is_none());
        assert_eq!(dup_fileset.extras.len(), 0);
    }

    #[test]
    fn test_process_a_duplicate_file_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators - returns ok for file operations
        let file_ops = MockFileOperationsOk;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        // FindCommand does not operate on the file, so it always returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_file_find() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators - returns ok for file operations
        let file_ops = MockFileOperationsOk;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        // FindCommand does not operate of the file, so it always returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_file_find_quiet() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.shared.quiet = true;
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators - returns ok for file operations
        let file_ops = MockFileOperationsOk;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        // FindCommand does not operate of the file, so it always returns Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_delete_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::Delete {
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsError;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_a_duplicate_delete() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::Delete {
            method: DuplicateSelectionMethod::Newest,
        };
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsOk;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_copy_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::Copy {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsError;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_a_duplicate_copy() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::Copy {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsOk;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_a_duplicate_move_badfilepath() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::Move {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsError;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_a_duplicate_move() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::Move {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };
        let mut multi = MultiProgress::new();
        // fake file
        let file_info = FileInfo {
            path: "xxx.xxx".to_string(),
            size: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        // use our mock file operators
        let file_ops = MockFileOperationsOk;
        let result =
            process_a_duplicate_file(&file_ops, &args, &file_info, "0000000000000000", &mut multi);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_duplicates_move() {
        let mut args = create_default_command_line_arguments();
        args.shared.dry_run = false;
        args.command = Commands::Move {
            location: "/bad/path".to_string(),
            method: DuplicateSelectionMethod::Newest,
            flatten: false,
            no_hash_folder: false,
            overwrite: false,
        };

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
        let result = process_duplicates(&file_ops, &args, &hash_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].hash, "testhashkey");
        assert!(result[0].keeper.is_some());
        assert_eq!(result[0].extras.len(), 1);
        assert_eq!(result[0].result, DuplicateResult::Moved);
    }

    #[test]
    fn test_terminal_guard() {
        // Create an instance of TerminalGuard that will be dropped when main exits
        let _guard = TerminalGuard;
        setup_terminal();
        drop(_guard);
    }

    #[test]
    fn test_print_banner() {
        setup_terminal();
        print_banner();
        reset_terminal();
    }
}
