use chrono::{DateTime, Utc};
use clap::Parser;
use indicatif::ProgressBar;
use md5::{self, Digest};
use std::io::Read;
use std::thread;
use std::time::UNIX_EPOCH;
use std::{collections::HashMap, time::Duration};

#[derive(Parser, Debug)]
#[command(name = "Dupefindr", version)]
#[command(about = "A tool to find duplicate files", long_about = None)]
#[command(propagate_version = true)]
struct Args {
    /// The directory to search for duplicates in.
    #[arg(short, long, default_value = ".")]
    path: String,

    /// Recursively search for duplicates
    #[arg(short, long)]
    recursive: bool,

    /// Display debug information
    #[arg(short, long, default_value = "true")]
    debug: bool,

    /// Include 0 byte files
    #[arg(long, default_value = "false")]
    include_zero_byte_files: bool,

    /// Dry run the program
    /// This will not delete or modify any files
    #[arg(long, default_value = "false")]
    dry_run: bool,

    /// Include hidden files
    #[arg(long, default_value = "false")]
    include_hidden_files: bool,
}

struct FileInfo {
    path: String,
    size: u64,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
}

fn main() {
    let args = get_command_line_arguments();

    start_search(&args);
}

fn get_command_line_arguments() -> Args {
    let args = Args::parse();
    if args.debug {
        println!("Searching for duplicates in: {}", args.path);
        if args.recursive {
            println!("Recursively searching for duplicates");
        }
        println!("Include 0 byte files: {}", args.include_zero_byte_files);
        println!("Dry run: {}", args.dry_run);
        println!("Include hidden files: {}", args.include_hidden_files);
    }
    args
}

fn start_search(args: &Args) {
    let bar = ProgressBar::new_spinner().with_message("Collecting files...");
    bar.enable_steady_tick(Duration::from_millis(100));
    // get the files in the directory
    let folder_path: String = args.path.clone();
    let mut hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
    let _files = get_files_in_directory(args,folder_path, &bar);
    if args.debug {
        bar.println(format!("Found {} files", _files.len()));
    }
    bar.set_message("Identifying duplicates...");
    for file in _files {
        let hash_string = get_hash_of_file(&file.path, &bar);
        if args.debug {
            bar.println(format!(
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
        if args.debug {
            thread::sleep(Duration::from_millis(500));
        }
    }
    // print the duplicates
    for (hash, files) in hash_map.iter() {
        if files.len() > 1 {
            bar.println(format!(
                "Found {} duplicates for hash: {}",
                files.len(),
                hash
            ));
            for file in files {
                bar.println(format!(
                    "File: {} [created: {}] [modified: {}] [{} bytes]",
                    file.path,
                    file.created_at.to_rfc2822(),
                    file.modified_at.to_rfc2822(),
                    file.size
                ));
            }
        }
    }
    bar.finish_and_clear();
}

fn get_files_in_directory(args: &Args, folder_path: String, bar: &ProgressBar) -> Vec<FileInfo> {
    let mut files: Vec<FileInfo> = Vec::new();
    let dir_path = std::path::Path::new(folder_path.as_str());
    let total_files = std::fs::read_dir(dir_path).unwrap().count();
    bar.println(format!("Searching in: {} - {} objects", folder_path, total_files));
    for entry in std::fs::read_dir(dir_path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            if !args.recursive {
                if args.debug {
                    bar.println(format!("Ignoring directory: {}", path.to_str().unwrap()));
                    thread::sleep(Duration::from_millis(500));
                }
                continue;
            }
            else {
                let sub_files = get_files_in_directory(args, path.to_str().unwrap().to_string(), bar);
                files.extend(sub_files);
            }
        }
        if path.is_file() {
            let hidden: bool;
            #[cfg(not(target_os = "windows"))]
            {
                hidden = path.file_name().unwrap().to_str().unwrap().starts_with(".");
            }
            #[cfg(target_os = "windows")]
            {
                if std::fs::metadata(&path).unwrap().file_attributes().hidden().unwrap() {
                    hidden = true;
                }
            }
            if args.include_hidden_files == false && hidden {
                if args.debug {
                    bar.println(format!("Ignoring hidden file: {}", path.to_str().unwrap()));
                    thread::sleep(Duration::from_millis(500));
                }
                continue;
            }
            let size = std::fs::metadata(&path).unwrap().len();
            let created_at = std::fs::metadata(&path).unwrap().created().unwrap();
            let modified_at = std::fs::metadata(&path).unwrap().modified().unwrap();
            // Convert SystemTime to chrono::DateTime<Utc>
            let created_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                + chrono::Duration::from_std(created_at.duration_since(UNIX_EPOCH).unwrap())
                    .unwrap();
            let modified_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                + chrono::Duration::from_std(modified_at.duration_since(UNIX_EPOCH).unwrap())
                    .unwrap();

            if size == 0 && !args.include_zero_byte_files {
                if args.debug {
                    bar.println(format!("Ignoring 0 byte file: {}", path.to_str().unwrap()));
                    thread::sleep(Duration::from_millis(500));
                }
                continue;
            }
            let file_info = FileInfo {
                path: path.to_str().unwrap().to_string(),
                size,
                created_at: created_at_utc_datetime,
                modified_at: modified_at_utc_datetime,
            };
            files.push(file_info);
            if args.debug {
                thread::sleep(Duration::from_millis(500));
            }
        }
    }

    files
}

fn get_hash_of_file(file_path: &String, _bar: &ProgressBar) -> String {
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
