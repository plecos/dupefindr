use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use glob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use md5::{self, Digest};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::UNIX_EPOCH;
use std::{collections::HashMap, time::Duration};
use std::{fs, thread};
use threadpool::ThreadPool;

static DEBUG_DELAY: u64 = 0;

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

#[derive(ValueEnum, Debug, Clone, PartialEq)]
enum DuplicateSelectionMethod {
    Newest,
    Oldest,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Commands {
    #[command(name = "find", about = "Find duplicate files")]
    FindDuplicates {},

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

#[derive(Debug, Clone)]
struct FileInfo {
    path: String,
    size: u64,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
}

fn main() {
    let args = get_command_line_arguments();

    match start_search(&args) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn get_command_line_arguments() -> Args {
    let args = Args::parse();
    if args.shared.debug {
        println!("Command: {:?}", args.command);
        println!("Searching for duplicates in: {}", args.shared.path);
        if args.shared.recursive {
            println!("Recursively searching for duplicates");
        }
        println!("Include empty files: {}", args.shared.include_empty_files);
        println!("Dry run: {}", args.shared.dry_run);
        println!("Include hidden files: {}", args.shared.include_hidden_files);
        println!("Verbose: {}", args.shared.verbose);
        println!("Quiet: {}", args.shared.quiet);
        println!("Wildcard: {}", args.shared.wildcard);
        println!("Exclusion wildcard: {}", args.shared.exclusion_wildcard);
        let default_parallelism_approx = num_cpus::get();
        println!("Available cpus: {}", default_parallelism_approx);
        println!();
    }

    args
}
fn start_search(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // get the files in the directory
    let folder_path: String = args.shared.path.clone();

    let _result = get_files_in_directory(args, folder_path, None);
    let _files = match _result {
        Ok(files) => files,
        Err(e) => {
            println!("Error: {}", e);
            return Err(e);
        }
    };
    if args.shared.verbose {
        println!("Found {} files", _files.len());
    }

    // identify the duplicates
    let full_hash_map = identify_duplicates(args, _files);
    let hash_map = process_duplicates(args, &full_hash_map);

    // print the duplicates
    let duplicates_found = hash_map.len();
    let mut duplicates_total_size: i64 = 0;
    for (hash, files) in hash_map.iter() {
        if args.shared.verbose {
            println!("Found {} duplicates for hash: {}", files.len(), hash);
        }
        for file in files {
            if args.shared.verbose {
                println!(
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
            println!();
        }
    }
    if duplicates_found == 0 {
        println!("No duplicates found");
    } else {
        println!(
            "Found {} duplicates with total size {}",
            duplicates_found,
            bytesize::ByteSize(duplicates_total_size as u64)
        );
    }

    Ok(())
}

fn get_files_in_directory(
    args: &Args,
    folder_path: String,
    multi: Option<&MultiProgress>,
) -> Result<Vec<FileInfo>, Box<dyn std::error::Error>> {
    let multi = match multi {
        Some(m) => m,
        None => &MultiProgress::new(),
    };
    let mut files: Vec<FileInfo> = Vec::new();
    //let dir_path = std::path::Path::new(folder_path.as_str());
    match fs::metadata(folder_path.as_str()) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(Box::<dyn std::error::Error>::from(
                    "The path provided is not a directory",
                ));
            }
        }
        Err(_) => {
            return Err(Box::from("The path provided is not a directory"));
        }
    }
    if args.shared.debug {
        let _ = multi.println(format!("Collecting objects in: {}", folder_path));
    }
    let entries = fs::read_dir(&folder_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
    if args.shared.debug {
        let _ = multi.println(format!("Finished collecting objects in: {}", folder_path));
    }

    let sty_folders = ProgressStyle::with_template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
        .unwrap()
        .progress_chars("##-");

    let sty_files = ProgressStyle::with_template("{bar:40.green/green} {pos:>7}/{len:7} {msg}")
        .unwrap()
        .progress_chars("##-");

    let sty_processing = ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .progress_chars("##-");

    // process directories first
    //let bar = multi.add(ProgressBar::new_spinner());
    let bar = if args.shared.quiet {
        ProgressBar::hidden()
    } else {
        multi.add(ProgressBar::new_spinner())
    };
    bar.set_style(sty_processing);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message("Processing objects");

    let mut folder_count = 0;
    let mut file_count = 0;
    let mut folders: Vec<PathBuf> = Vec::new();
    let workers = num_cpus::get();
    let pool = ThreadPool::new(workers);
    let (tx, rx) = channel();
    let files_count = entries.len();

    if args.shared.debug {
        let _ = multi.println(format!("Iterating entries: {}", folder_path));
    }
    // use thread pool to optimize the process
    for entry in entries.iter() {
        let tx = tx.clone();
        let entry = entry.clone();
        pool.execute(move || {
            let is_dir = entry.is_dir();
            tx.send((entry, is_dir)).unwrap();
        });
    }
    if args.shared.debug {
        let _ = multi.println(format!("Completed iterating entries: {}", folder_path));
    }

    // wait for the jobs to complete, and process the result
    rx.iter().take(files_count).for_each(|(entry, is_dir)| {
        if is_dir {
            folder_count += 1;
            folders.push(entry.clone());
        } else {
            file_count += 1;
        }
    });

    bar.finish_and_clear();
    multi.remove(&bar);

    let bar2 = if args.shared.quiet {
        ProgressBar::hidden()
    } else {
        multi.add(ProgressBar::new(folder_count as u64))
    };
    bar2.set_style(sty_folders);

    for fld in folders.iter() {
        bar2.set_message(format!("Folder {}", fld.to_str().unwrap().to_string()));
        let hidden: bool;
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
                    let _ = multi.println(format!(
                        "Ignoring hidden directory: {}",
                        fld.file_name().unwrap().to_str().unwrap()
                    ));
                }
                bar2.inc(1);
                continue;
            }
        }

        if !args.shared.recursive {
            if args.shared.verbose {
                let _ = multi.println(format!(
                    "Ignoring directory: {}",
                    fld.file_name().unwrap().to_str().unwrap()
                ));
            }
        } else {
            let path = fld.as_path();

            let sub_files =
                get_files_in_directory(args, path.to_str().unwrap().to_string(), Some(multi))?;
            files.extend(sub_files);
        }
        bar2.inc(1);
    }
    bar2.finish_and_clear();
    multi.remove(&bar2);

    let bar2 = if args.shared.quiet {
        ProgressBar::hidden()
    } else {
        multi.add(ProgressBar::new(file_count as u64))
    };
    bar2.set_style(sty_files);

    for entry in entries.iter() {
        let path = entry.as_path();
        let _ = bar2.set_message(format!("Processing: {}", path.to_str().unwrap()));

        if path.is_file() {
            // determine if the file matches the wildcard
            let wildcard_pattern = glob::Pattern::new(&args.shared.wildcard)?;
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
            if args.shared.exclusion_wildcard.len() > 0 {
                let exclusion_wildcard_pattern =
                    glob::Pattern::new(&args.shared.exclusion_wildcard)?;
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
                if args.shared.verbose {
                    let _ =
                        multi.println(format!("Ignoring hidden file: {}", path.to_str().unwrap()));
                }
                if args.shared.debug {
                    thread::sleep(Duration::from_millis(DEBUG_DELAY));
                }
                bar2.inc(1);
                continue;
            }
            let meta = std::fs::metadata(&path).unwrap();
            let size = meta.len();
            if size == 0 && !args.shared.include_empty_files {
                if args.shared.verbose {
                    let _ =
                        multi.println(format!("Ignoring empty file: {}", path.to_str().unwrap()));
                }
                if args.shared.debug {
                    thread::sleep(Duration::from_millis(DEBUG_DELAY));
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

    bar2.finish_and_clear();
    Ok(files)
}

fn identify_duplicates(args: &Args, files: Vec<FileInfo>) -> HashMap<String, Vec<FileInfo>> {
    let mut hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
    let multi = MultiProgress::new();
    let workers = num_cpus::get();

    let sty_dupes =
        ProgressStyle::with_template("ETA {eta} {bar:40.yellow/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-");
    let sty_processing = ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .progress_chars("##-");

    let bar = if args.shared.quiet {
        ProgressBar::hidden()
    } else {
        multi.add(ProgressBar::new(files.len() as u64))
    };
    bar.set_style(sty_dupes);
    let bar2 = if args.shared.quiet {
        ProgressBar::hidden()
    } else {
        multi.add(ProgressBar::new_spinner())
    };
    bar2.enable_steady_tick(Duration::from_millis(100));
    bar2.set_style(sty_processing);
    bar2.set_message("Identifying duplicates...");

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

        pool.execute(move || {
            let hash_string = get_hash_of_file(&file_path, &bar);
            tx.send((hash_string, file)).unwrap();
        });
    }

    // wait for the jobs to complete, and process the result
    rx.iter().take(files_count).for_each(|(hash_string, file)| {
        if args.shared.verbose {
            let _ = multi.println(format!(
                "File: {} [{} bytes] [hash: {}]",
                file.path, file.size, hash_string
            ));
        }
        if !hash_map.contains_key(&hash_string) {
            let mut vec = Vec::new();
            vec.push(file);
            hash_map.insert(hash_string, vec);
        } else {
            let vec = hash_map.get_mut(&hash_string).unwrap();
            vec.push(file);
        }
        bar.inc(1);
    });

    bar.finish_and_clear();
    bar2.finish_and_clear();

    multi.remove(&bar2);
    multi.remove(&bar);
    multi.clear().unwrap();

    hash_map
}

fn process_duplicates(
    args: &Args,
    hash_map: &HashMap<String, Vec<FileInfo>>,
) -> HashMap<String, Vec<FileInfo>> {
    let mut new_hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
    let workers = num_cpus::get();

    let multi = MultiProgress::new();
    let sty_processing = ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .progress_chars("##-");

    let sty_dupes =
        ProgressStyle::with_template("ETA {eta} {bar:40.yellow/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-");

    let bar = if args.shared.quiet {
        ProgressBar::hidden()
    } else {
        multi.add(ProgressBar::new(hash_map.len() as u64))
    };
    bar.set_style(sty_dupes);
    let bar2 = if args.shared.quiet {
        ProgressBar::hidden()
    } else {
        multi.add(ProgressBar::new_spinner())
    };
    bar2.enable_steady_tick(Duration::from_millis(100));
    bar2.set_style(sty_processing);
    bar2.set_message("Processing duplicates...");

    for (hash, files) in hash_map.iter() {
        if files.len() > 1 {
            new_hash_map.insert(hash.clone(), files.clone());
            // let pool = ThreadPool::new(workers);
            // let (tx, rx) = channel();
            // let files_count = files.len();
            match &args.command {
                Commands::FindDuplicates {} => {
                    if args.shared.debug {
                        let _ = multi.println(format!("FindDuplicates: {}", hash));
                    }
                    continue;
                }
                Commands::CopyDuplicates { location, method } => {
                    if args.shared.debug {
                        let _ = multi.println(format!("CopyDuplicates: {} to {}", hash, location));
                    }
                    // get the list of files to copy
                    let (keeper, selected_files): (Option<FileInfo>, Vec<FileInfo>) =
                        select_duplicate_files(method.clone(), files);
                    if keeper.is_none() {
                        if args.shared.debug {
                            let _ = multi.println("keeper is none");
                        }
                        continue;
                    }
                    if args.shared.debug {
                        let _ = multi.println(format!("Selected File: {}", keeper.unwrap().path));
                    }
                    // copy the files
                    for file in selected_files {
                        let source = &file.path;
                        let file_name =
                            Path::new(&file.path).file_name().unwrap().to_str().unwrap();
                        let destination = format!("{}/{}", location, file_name);

                        if !args.shared.dry_run {
                            if args.shared.verbose {
                                let _ = multi
                                    .println(format!("Copying: {} to {}", source, destination));
                            }
                            if let Err(e) = fs::copy(source, destination.clone()) {
                                eprintln!("Failed to copy {} to {}: {}", source, destination, e);
                            }
                        } else {
                            if args.shared.verbose {
                                let _ = multi.println(format!(
                                    "Dry run: Would copy {} to {}",
                                    source, destination
                                ));
                            }
                        }
                    }
                    continue;
                }
                Commands::MoveDuplicates { location, method } => {
                    if args.shared.debug {
                        let _ = multi.println(format!("MoveDuplicates: {} to {}", hash, location));
                    }
                    continue;
                }
                Commands::DeleteDuplicates { method } => {
                    if args.shared.debug {
                        let _ = multi.println(format!("DeleteDuplicates: {}", hash));
                    }
                    continue;
                }
            }
        }
        bar.inc(1);
    }

    bar.finish_and_clear();
    bar2.finish_and_clear();
    multi.remove(&bar2);
    multi.remove(&bar);
    multi.clear().unwrap();
    new_hash_map
}

fn get_hash_of_file(file_path: &str, _bar: &ProgressBar) -> String {
    let mut file = std::fs::File::open(file_path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    return get_md5_hash(&buffer);
}

fn get_md5_hash(buffer: &Vec<u8>) -> String {
    let mut hasher = md5::Md5::new();
    hasher.update(&buffer);
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

fn select_duplicate_files(
    method: DuplicateSelectionMethod,
    files: &Vec<FileInfo>,
) -> (Option<FileInfo>, Vec<FileInfo>) {
    if files.is_empty() {
        return (None, vec![]);
    }
    match method {
        DuplicateSelectionMethod::Newest => {
            // keep the newest file, so return all other files
            let mut sorted_files = files.clone();
            sorted_files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
            let keeper = sorted_files.swap_remove(0);
            (Some(keeper), sorted_files)
        }
        DuplicateSelectionMethod::Oldest => {
            // keep the oldest file, so return all other files
            let mut sorted_files = files.clone();
            sorted_files.sort_by(|a, b| a.modified_at.cmp(&b.modified_at));
            let keeper = sorted_files.swap_remove(0);
            (Some(keeper), sorted_files)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_default_command_line_arguments() -> Args {
        let shared_options = SharedOptions {
            path: "data".to_string(),
            recursive: false,
            debug: true,
            include_empty_files: false,
            dry_run: false,
            include_hidden_files: false,
            verbose: true,
            quiet: false,
            wildcard: "*".to_string(),
            exclusion_wildcard: "".to_string(),
        };
        let s1 = shared_options.clone();
        let args = Args {
            shared: s1,
            command: Commands::DeleteDuplicates {
                method: DuplicateSelectionMethod::Newest,
            },
        };
        args
    }

    #[test]
    fn test_get_files_in_directory() {
        let args = create_default_command_line_arguments();
        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files_in_directory_wildcard() {
        let mut args = create_default_command_line_arguments();
        args.shared.wildcard = "*testdupe*.txt".to_string();

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 4);
    }

    #[test]
    fn test_get_files_in_directory_exclusion_wildcard() {
        let mut args = create_default_command_line_arguments();
        args.shared.exclusion_wildcard = "*testdupe*.txt".to_string();

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_get_files_in_directory_include_empty() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_empty_files = true;

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 7);
    }

    #[test]
    fn test_get_files_in_directory_include_hidden() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_hidden_files = true;

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 6);
    }

    #[test]
    fn test_get_files_in_directory_include_all_files() {
        let mut args = create_default_command_line_arguments();
        args.shared.include_hidden_files = true;
        args.shared.include_empty_files = true;

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 8);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 16);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive_with_hidden() {
        let mut args = create_default_command_line_arguments();
        args.shared.recursive = true;
        args.shared.include_hidden_files = true;

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        assert_eq!(files.len(), 18);
    }

    #[test]
    fn test_get_files_in_directory_bad_path() {
        let mut args = create_default_command_line_arguments();
        args.shared.path = "badpath!!!".to_string();

        let result = get_files_in_directory(&args, "badpath!!!".to_string(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_hash_of_file() {
        let hash = get_hash_of_file("data//testdupe1.txt", &ProgressBar::new_spinner());
        assert_eq!(hash, "8c91214730e59f67bd46d1855156e762");
    }

    #[test]
    #[should_panic]
    fn test_get_hash_of_file_bad_path() {
        let hash = get_hash_of_file("data//testdupe1-notfound.txt", &ProgressBar::new_spinner());
        assert_eq!(hash, "8c91214730e59f67bd46d1855156e762");
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

        let result = start_search(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_bad_path() {
        let mut args = create_default_command_line_arguments();
        args.shared.path = "data-badpath!!!".to_string();

        let result = start_search(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_identify_duplicates() {
        let args = create_default_command_line_arguments();

        let files = get_files_in_directory(&args, "data".to_string(), None).unwrap();
        let hash_map = identify_duplicates(&args, files);
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
    fn test_select_duplicate_files_newest() {

        let mut files = Vec::new();
        files.push(FileInfo {
            path: "data/testdupe1.txt".to_string(),
            size: 1024,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        });
        files.push(FileInfo {
            path: "data/testdupe2.txt".to_string(),
            size: 1024,
            created_at: Utc::now()-chrono::Duration::days(1),
            modified_at: Utc::now()-chrono::Duration::days(1),
        });
        files.push(FileInfo {
            path: "data/testdupe3.txt".to_string(),
            size: 1024,
            created_at: Utc::now()-chrono::Duration::days(2),
            modified_at: Utc::now()-chrono::Duration::days(2),
        });
        let (keeper, selected_files) = select_duplicate_files(
            DuplicateSelectionMethod::Newest,
            &files,
        );
        assert_eq!(keeper.is_some(),true);
        assert_eq!(keeper.unwrap().path, "data/testdupe1.txt".to_string());
        assert_eq!(selected_files.len(), 2);
        // the order of the selected files is not guarenteed, so we check to see if our files are just in there somewhere
        let file1 = selected_files.iter().find(|file| file.path == "data/testdupe3.txt");
        let file2 = selected_files.iter().find(|file| file.path == "data/testdupe2.txt");

        assert!(file1.is_some());
        assert!(file2.is_some());
        

    }

    #[test]
    fn test_select_duplicate_files_oldest() {

        let mut files = Vec::new();
        files.push(FileInfo {
            path: "data/testdupe1.txt".to_string(),
            size: 1024,
            created_at: Utc::now(),
            modified_at: Utc::now(),
        });
        files.push(FileInfo {
            path: "data/testdupe2.txt".to_string(),
            size: 1024,
            created_at: Utc::now()-chrono::Duration::days(1),
            modified_at: Utc::now()-chrono::Duration::days(1),
        });
        files.push(FileInfo {
            path: "data/testdupe3.txt".to_string(),
            size: 1024,
            created_at: Utc::now()-chrono::Duration::days(2),
            modified_at: Utc::now()-chrono::Duration::days(2),
        });
        let (keeper, selected_files) = select_duplicate_files(
            DuplicateSelectionMethod::Oldest,
            &files,
        );
        assert_eq!(keeper.is_some(),true);
        assert_eq!(keeper.unwrap().path, "data/testdupe3.txt".to_string());
        assert_eq!(selected_files.len(), 2);
        
        // the order of the selected files is not guarenteed, so we check to see if our files are just in there somewhere
        let file1 = selected_files.iter().find(|file| file.path == "data/testdupe1.txt");
        let file2 = selected_files.iter().find(|file| file.path == "data/testdupe2.txt");

        assert!(file1.is_some());
        assert!(file2.is_some());
        

    }

    #[test]
    fn test_select_duplicate_files_empty_files() {
        let files = Vec::new();
        let (keeper, selected_files) = select_duplicate_files(
            DuplicateSelectionMethod::Oldest,
            &files,
        );
        assert_eq!(keeper.is_none(), true);
        assert_eq!(selected_files.len(), 0);

        
    }
}
