use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::ffi::c_void;

use clap::{Parser, ValueEnum};
use crossbeam_channel::{bounded, Sender};

#[cfg(windows)]
const MAX_PREFERRED_LENGTH: u32 = u32::MAX;

#[cfg(windows)]
#[repr(C)]
struct SHARE_INFO_1 {
    shi1_netname: *mut u16,
    shi1_type: u32,
    shi1_remark: *mut u16,
}

#[cfg(windows)]
#[link(name = "Netapi32")]
extern "system" {
    fn NetShareEnum(
        servername: *const u16,
        level: u32,
        bufptr: *mut *mut u8,
        prefmaxlen: u32,
        entriesread: *mut u32,
        totalentries: *mut u32,
        resume_handle: *mut u32,
    ) -> u32;

    fn NetApiBufferFree(buffer: *mut c_void) -> u32;
}

const STYPE_DISKTREE: u32 = 0;
const STYPE_SPECIAL: u32 = 0x8000_0000;



#[derive(Debug, Clone, ValueEnum)]
enum Mode {
    
    File,
    
    Folder,
    
    All,
}


#[derive(Parser, Debug)]
#[command(
    name = "search",
    author,
    version,
    about,
    long_about = None,
    help_template = "\
{before-help}{name} {version}
{author-with-newline}
{about-section}
Usage:
  search.exe <MODE> <PATHS>... <TERM> [OPTIONS]

Modes:
  file      Search only files
  folder    Search only folders
  all       Search files and folders

{all-args}{after-help}
"
)]
struct Cli {
    
    mode: Mode,

    
    
    
    
    #[arg(required = true, num_args = 2..)]
    paths_and_term: Vec<String>,

    
    #[arg(short = 'd', long, default_value_t = 5)]
    depth: usize,

    
    #[arg(short = 't', long, default_value_t = 20, value_parser = clap::value_parser!(u16).range(1..=512))]
    threads: u16,

    
    #[arg(short = 'q', long)]
    quiet: bool,

    
    #[arg(long)]
    no_color: bool,
}



#[derive(Clone)]
struct Job {
    path: PathBuf,
    depth: usize,
}



fn main() {
    let cli = Cli::parse();

    
    let raw = cli.paths_and_term;
    if raw.len() < 2 {
        eprintln!("[ERROR] Debes indicar al menos una ruta y un termino de busqueda.");
        std::process::exit(1);
    }
    let term = raw.last().unwrap().to_lowercase();
    let input_roots: Vec<String> = raw[..raw.len() - 1].to_vec();

    let worker_count = cli.threads as usize;
    let max_depth = cli.depth;
    let quiet = cli.quiet;
    let no_color = cli.no_color;
    let mode = cli.mode.clone();

    
    
    
    
    
    
    
    let use_color = !quiet && !no_color
        && std::env::var_os("NO_COLOR").is_none()
        && stdout_supports_ansi();

    
    let mut roots: Vec<PathBuf> = Vec::new();
    for input in &input_roots {
        match resolve_roots(input, quiet) {
            Ok(mut r) => roots.append(&mut r),
            Err(e) => {
                eprintln!("[ERROR] {}", e);
                std::process::exit(1);
            }
        }
    }

    if roots.is_empty() {
        eprintln!("[ERROR] No se encontraron rutas validas donde buscar.");
        std::process::exit(1);
    }

    
    const SEP: &str = "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";
    if !quiet {
        let mode_str = match mode {
            Mode::File => "file",
            Mode::Folder => "folder",
            Mode::All => "all",
        };
        println!("Searcher \u{00b7} jss");
        println!("{}", SEP);
        println!("Mode:       {}", mode_str);
        println!("Pattern:    {}", term);
        println!("Depth:      {}", max_depth);
        println!("Threads:    {}", worker_count);
        println!("Roots:      {}", roots.len());
        println!("{}", SEP);
        println!("Targets:");
        for (i, r) in roots.iter().enumerate() {
            println!("  [{}] {}", i + 1, r.display());
        }
        println!("{}", SEP);
        println!("Results:");
    }

    
    let (tx, rx) = bounded::<Job>(worker_count * 64);
    let active = Arc::new(AtomicUsize::new(roots.len()));

    
    for root in &roots {
        if tx.send(Job { path: root.clone(), depth: 0 }).is_err() {
            eprintln!("[ERROR] No se pudo encolar: {}", root.display());
        }
    }

    let warnings: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let color_flag = Arc::new(AtomicBool::new(use_color));
    let mut threads_vec = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let rx = rx.clone();
        let tx2 = tx.clone();
        let term = term.clone();
        let active2 = Arc::clone(&active);
        let mode2 = mode.clone();
        let warnings2 = Arc::clone(&warnings);
        let color2 = Arc::clone(&color_flag);

        threads_vec.push(thread::spawn(move || loop {
            let job = match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(job) => job,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if active2.load(Ordering::SeqCst) == 0 {
                        return;
                    }
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            };

            let entries = match fs::read_dir(&job.path) {
                Ok(e) => e,
                Err(_) => {
                    if !quiet {
                        if let Ok(mut w) = warnings2.lock() {
                            w.push(format!("{}", job.path.display()));
                        }
                    }
                    active2.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
            };

            for entry in entries.flatten() {
                
                let ft = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                let name_matches = name.contains(&term);

                if ft.is_dir() && !ft.is_symlink() {
                    
                    #[cfg(windows)]
                    if is_reparse_point(&path) {
                        continue;
                    }

                    
                    if name_matches {
                        match mode2 {
                            Mode::Folder | Mode::All => {
                                print_result(color2.load(Ordering::Relaxed), Tag::Folder, &path);
                            }
                            Mode::File => {}
                        }
                    }

                    
                    if job.depth < max_depth {
                        active2.fetch_add(1, Ordering::SeqCst);
                        
                        if tx2.send(Job { path, depth: job.depth + 1 }).is_err() {
                            active2.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                } else if ft.is_file() {
                    if name_matches {
                        match mode2 {
                            Mode::File | Mode::All => {
                                print_result(color2.load(Ordering::Relaxed), Tag::File, &path);
                            }
                            Mode::Folder => {}
                        }
                    }
                }
            }

            active2.fetch_sub(1, Ordering::SeqCst);
        }));
    }

    
    drop(tx);

    for t in threads_vec {
        let _ = t.join();
    }

    println!();

    
    if !quiet {
        if let Ok(mut w) = warnings.lock() {
            if !w.is_empty() {
                w.sort_unstable();
                w.dedup();
                println!("Inaccessible ({}):", w.len());
                for path in w.iter() {
                    println!("  {}", path);
                }
                println!();
            }
        }
    }
}




enum Tag { File, Folder }

fn print_result(color: bool, tag: Tag, path: &std::path::Path) {
    if color {
        match tag {
            
            Tag::File   => println!("\x1b[38;5;136m[FILE]\x1b[0m   {}", path.display()),
            Tag::Folder => println!("\x1b[38;5;67m[FOLDER]\x1b[0m {}", path.display()),
        }
    } else {
        match tag {
            Tag::File   => println!("[FILE]   {}", path.display()),
            Tag::Folder => println!("[FOLDER] {}", path.display()),
        }
    }
}




fn stdout_supports_ansi() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        
        unsafe {
            let handle = std::io::stdout().as_raw_handle();
            if handle.is_null() || handle == usize::MAX as *mut _ {
                return false;
            }
            
            let file_type = GetFileType(handle as isize);
            if file_type != 0x0002 {
                return false; 
            }
            let mut mode: u32 = 0;
            if GetConsoleMode(handle as isize, &mut mode) == 0 {
                return false;
            }
            const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
            if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
                return true; 
            }
            
            SetConsoleMode(handle as isize, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }
    #[cfg(not(windows))]
    {
        extern "C" { fn isatty(fd: i32) -> i32; }
        unsafe { isatty(1) == 1 }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetConsoleMode(h_console: isize, lp_mode: *mut u32) -> i32;
    fn SetConsoleMode(h_console: isize, dw_mode: u32) -> i32;
    fn GetFileType(h_file: isize) -> u32;
}



fn resolve_roots(input: &str, quiet: bool) -> Result<Vec<PathBuf>, String> {
    if is_unc_server_only(input) {
        let shares = enumerate_server_shares(input)?;
        let mut result = Vec::new();
        for share in shares {
            let path = PathBuf::from(&share);
            
            match fs::read_dir(&path) {
                Ok(_) => result.push(path),
                Err(e) => {
                    if !quiet {
                        eprintln!("[WARN] Sin permisos o inaccesible: {} ({})", share, e);
                    }
                }
            }
        }
        Ok(result)
    } else {
        Ok(vec![PathBuf::from(input)])
    }
}

fn is_unc_server_only(input: &str) -> bool {
    let trimmed = input.trim().trim_end_matches(['\\', '/']);
    if !trimmed.starts_with(r"\\") {
        return false;
    }
    let without_prefix = &trimmed[2..];
    !without_prefix.is_empty() && !without_prefix.contains('\\') && !without_prefix.contains('/')
}


#[cfg(windows)]
fn is_reparse_point(path: &std::path::Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn enumerate_server_shares(server: &str) -> Result<Vec<String>, String> {
    const ERROR_MORE_DATA: u32 = 234;

    let normalized = server.trim().trim_end_matches(['\\', '/']);
    let server_wide: Vec<u16> = normalized.encode_utf16().chain(std::iter::once(0)).collect();
    let mut resume_handle: u32 = 0;
    let mut result = Vec::new();

    loop {
        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut entries_read: u32 = 0;
        let mut total_entries: u32 = 0;

        let status = unsafe {
            NetShareEnum(
                server_wide.as_ptr(),
                1,
                &mut buffer,
                MAX_PREFERRED_LENGTH,
                &mut entries_read,
                &mut total_entries,
                &mut resume_handle,
            )
        };

        if status != 0 && status != ERROR_MORE_DATA {
            if !buffer.is_null() {
                unsafe { NetApiBufferFree(buffer as *mut c_void) };
            }
            return Err(format!(
                "No se pudieron enumerar los recursos compartidos de {}. Codigo NetShareEnum: {}",
                normalized, status
            ));
        }

        if !buffer.is_null() && entries_read > 0 {
            unsafe {
                let shares = std::slice::from_raw_parts(buffer as *const SHARE_INFO_1, entries_read as usize);

                for share in shares {
                    let share_type = share.shi1_type;
                    
                    let is_disk = (share_type & !STYPE_SPECIAL) == STYPE_DISKTREE;
                    let is_special = (share_type & STYPE_SPECIAL) != 0;

                    if !is_disk || is_special || share.shi1_netname.is_null() {
                        continue;
                    }

                    let name = wide_ptr_to_string(share.shi1_netname);

                    
                    if name.is_empty() || name.ends_with('$') {
                        continue;
                    }

                    result.push(format!(r"{}\{}", normalized, name));
                }

                NetApiBufferFree(buffer as *mut c_void);
            }
        }

        if status != ERROR_MORE_DATA {
            break;
        }
    }

    result.sort_unstable();
    result.dedup();
    Ok(result)
}

#[cfg(windows)]
unsafe fn wide_ptr_to_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
}

#[cfg(not(windows))]
fn enumerate_server_shares(_server: &str) -> Result<Vec<String>, String> {
    Err("La enumeracion automatica de recursos compartidos SMB solo esta disponible en Windows.".to_string())
}
