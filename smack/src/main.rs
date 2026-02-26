mod detector;
mod sensor;

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;

const SOCKET_PATH: &str = "/tmp/smack.sock";

extern "C" {
    fn geteuid() -> u32;
    fn chmod(path: *const i8, mode: u32) -> i32;
}

fn main() {
    if unsafe { geteuid() } != 0 {
        eprintln!("smack requires root for accelerometer access");
        eprintln!("usage: sudo smack");
        std::process::exit(1);
    }

    // Clean up stale socket from previous run
    let _ = std::fs::remove_file(SOCKET_PATH);

    let listener = UnixListener::bind(SOCKET_PATH).unwrap_or_else(|e| {
        eprintln!("smack: failed to bind socket {SOCKET_PATH}: {e}");
        std::process::exit(1);
    });

    // Allow non-root neovim to connect
    let path_cstr = std::ffi::CString::new(SOCKET_PATH).unwrap();
    unsafe {
        chmod(path_cstr.as_ptr(), 0o777);
    }

    listener.set_nonblocking(true).ok();

    let clients: Arc<Mutex<Vec<UnixStream>>> = Arc::new(Mutex::new(Vec::new()));

    // Accept connections in background
    let clients_accept = clients.clone();
    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, _)) => {
                eprintln!("smack: client connected");
                stream.set_nonblocking(false).ok();
                clients_accept.lock().unwrap().push(stream);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                eprintln!("smack: accept error: {e}");
                thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    });

    // Start sensor on dedicated thread (runs CFRunLoop)
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Err(e) = sensor::start(tx) {
            eprintln!("smack: sensor error: {e}");
            std::process::exit(1);
        }
    });

    // Give sensor a moment to initialize
    thread::sleep(std::time::Duration::from_millis(200));

    eprintln!("smack: socket at {SOCKET_PATH}");
    eprintln!("smack: waiting for impacts... (ctrl+c to quit)");

    // Detection loop
    let mut det = detector::Detector::new();
    let mut hit_count: u64 = 0;

    while let Ok(sample) = rx.recv() {
        if let Some(event) = det.process(sample.x, sample.y, sample.z) {
            hit_count += 1;

            let json = format!(
                r#"{{"severity":"{}","amplitude":{:.4},"undos":{}}}"#,
                event.severity.as_str(),
                event.amplitude,
                event.severity.undos(),
            );

            // Print to stdout
            eprintln!(
                "smack: hit #{} [{}  amp={:.4}g  undos={}]",
                hit_count,
                event.severity.as_str(),
                event.amplitude,
                event.severity.undos(),
            );
            println!("{json}");
            std::io::stdout().flush().ok();

            // Broadcast to connected Neovim instances
            let mut clients = clients.lock().unwrap();
            clients.retain_mut(|stream| {
                match writeln!(stream, "{}", json) {
                    Ok(_) => {
                        stream.flush().ok();
                        true
                    }
                    Err(_) => {
                        eprintln!("smack: client disconnected");
                        false
                    }
                }
            });
        }
    }
}
