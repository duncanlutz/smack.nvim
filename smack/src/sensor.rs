/// Apple Silicon accelerometer reader via IOKit HID.
/// Ported from https://github.com/olvvier/apple-silicon-accelerometer
///
/// Accesses the Bosch BMI286 IMU through the AppleSPU HID interface.
/// Requires root privileges (sudo).

use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::mpsc;

// ── Core Foundation type aliases ────────────────────────────────────────────

type CFAllocatorRef = *const c_void;
type CFStringRef = *const c_void;
type CFNumberRef = *const c_void;
type CFTypeRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFMutableDictionaryRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFIndex = isize;

// ── IOKit type aliases ──────────────────────────────────────────────────────

type IOReturn = i32;
type MachPort = u32;
type IOIterator = u32;
type IOObject = u32;
type IOHIDDeviceRef = *mut c_void;

// ── Constants ───────────────────────────────────────────────────────────────

const KERN_SUCCESS: IOReturn = 0;
const K_IO_MAIN_PORT_DEFAULT: MachPort = 0;
const K_CF_ALLOCATOR_DEFAULT: CFAllocatorRef = std::ptr::null();
const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;
const K_CF_NUMBER_SINT32_TYPE: CFIndex = 3;

// HID usage identifiers for the accelerometer
const PAGE_VENDOR: i32 = 0xFF00;
const USAGE_ACCEL: i32 = 3;

// HID report format (Bosch BMI286 IMU)
const IMU_REPORT_LEN: usize = 22;
const IMU_DATA_OFF: usize = 6;
const ACCEL_SCALE: f64 = 65536.0; // Q16 fixed-point -> g
const IMU_DECIMATION: u32 = 8; // keep 1 in 8 samples (~800Hz -> ~100Hz)
const REPORT_BUF_SZ: usize = 4096;
const REPORT_INTERVAL_US: i32 = 1000;

// ── Public types ────────────────────────────────────────────────────────────

pub struct Sample {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

// ── FFI bindings ────────────────────────────────────────────────────────────

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const i8) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingServices(
        mainPort: MachPort,
        matching: CFDictionaryRef,
        existing: *mut IOIterator,
    ) -> IOReturn;
    fn IOIteratorNext(iterator: IOIterator) -> IOObject;
    fn IORegistryEntryCreateCFProperty(
        entry: IOObject,
        key: CFStringRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> CFTypeRef;
    fn IORegistryEntrySetCFProperty(
        entry: IOObject,
        name: CFStringRef,
        property: CFTypeRef,
    ) -> IOReturn;
    fn IOObjectRelease(object: IOObject) -> IOReturn;
    fn IOHIDDeviceCreate(allocator: CFAllocatorRef, service: IOObject) -> IOHIDDeviceRef;
    fn IOHIDDeviceOpen(device: IOHIDDeviceRef, options: u32) -> IOReturn;
    fn IOHIDDeviceRegisterInputReportCallback(
        device: IOHIDDeviceRef,
        report: *mut u8,
        reportLength: CFIndex,
        callback: unsafe extern "C" fn(
            context: *mut c_void,
            result: IOReturn,
            sender: *mut c_void,
            report_type: u32,
            report_id: u32,
            report: *mut u8,
            report_length: CFIndex,
        ),
        context: *mut c_void,
    );
    fn IOHIDDeviceScheduleWithRunLoop(
        device: IOHIDDeviceRef,
        runLoop: CFRunLoopRef,
        runLoopMode: CFStringRef,
    );
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cStr: *const i8,
        encoding: u32,
    ) -> CFStringRef;
    fn CFNumberCreate(
        allocator: CFAllocatorRef,
        theType: CFIndex,
        valuePtr: *const c_void,
    ) -> CFNumberRef;
    fn CFNumberGetValue(number: CFNumberRef, theType: CFIndex, valuePtr: *mut c_void) -> bool;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, returnAfterSourceHandled: bool) -> i32;
    fn CFRelease(cf: CFTypeRef);

    static kCFRunLoopDefaultMode: CFStringRef;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn cfstr(s: &str) -> CFStringRef {
    let cstr = CString::new(s).unwrap();
    unsafe { CFStringCreateWithCString(K_CF_ALLOCATOR_DEFAULT, cstr.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
}

fn cfnum32(val: i32) -> CFNumberRef {
    unsafe {
        CFNumberCreate(
            K_CF_ALLOCATOR_DEFAULT,
            K_CF_NUMBER_SINT32_TYPE,
            &val as *const i32 as *const c_void,
        )
    }
}

fn prop_int(service: IOObject, key: &str) -> Option<i32> {
    let cf_key = cfstr(key);
    let cf_val = unsafe { IORegistryEntryCreateCFProperty(service, cf_key, K_CF_ALLOCATOR_DEFAULT, 0) };
    unsafe { CFRelease(cf_key) };

    if cf_val.is_null() {
        return None;
    }

    let mut val: i32 = 0;
    let ok =
        unsafe { CFNumberGetValue(cf_val, K_CF_NUMBER_SINT32_TYPE, &mut val as *mut i32 as *mut c_void) };
    unsafe { CFRelease(cf_val) };

    if ok {
        Some(val)
    } else {
        None
    }
}

// ── HID report callback ────────────────────────────────────────────────────

struct CallbackContext {
    tx: mpsc::Sender<Sample>,
    decimation_counter: u32,
}

unsafe extern "C" fn accel_report_callback(
    context: *mut c_void,
    _result: IOReturn,
    _sender: *mut c_void,
    _report_type: u32,
    _report_id: u32,
    report: *mut u8,
    report_length: CFIndex,
) {
    if report_length as usize != IMU_REPORT_LEN {
        return;
    }

    let ctx = &mut *(context as *mut CallbackContext);

    // Decimation: keep 1 in 8 reports (~800Hz -> ~100Hz)
    ctx.decimation_counter += 1;
    if ctx.decimation_counter < IMU_DECIMATION {
        return;
    }
    ctx.decimation_counter = 0;

    let data = std::slice::from_raw_parts(report, IMU_REPORT_LEN);
    let o = IMU_DATA_OFF;

    let x_raw = i32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let y_raw = i32::from_le_bytes([data[o + 4], data[o + 5], data[o + 6], data[o + 7]]);
    let z_raw = i32::from_le_bytes([data[o + 8], data[o + 9], data[o + 10], data[o + 11]]);

    let sample = Sample {
        x: x_raw as f64 / ACCEL_SCALE,
        y: y_raw as f64 / ACCEL_SCALE,
        z: z_raw as f64 / ACCEL_SCALE,
    };

    // Non-blocking send — if the receiver is full/gone, just drop the sample
    let _ = ctx.tx.send(sample);
}

// ── Sensor initialization ───────────────────────────────────────────────────

/// Wake the SPU drivers so they start producing HID reports.
fn wake_spu_drivers() -> Result<(), String> {
    let class_name = CString::new("AppleSPUHIDDriver").unwrap();
    let matching = unsafe { IOServiceMatching(class_name.as_ptr()) };
    if matching.is_null() {
        return Err("failed to create matching dict for AppleSPUHIDDriver".into());
    }

    let mut iterator: IOIterator = 0;
    let kr = unsafe {
        IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching as CFDictionaryRef, &mut iterator)
    };
    if kr != KERN_SUCCESS {
        return Err(format!("IOServiceGetMatchingServices failed for drivers: {kr}"));
    }

    let props = [
        ("SensorPropertyReportingState", 1),
        ("SensorPropertyPowerState", 1),
        ("ReportInterval", REPORT_INTERVAL_US),
    ];

    loop {
        let svc = unsafe { IOIteratorNext(iterator) };
        if svc == 0 {
            break;
        }
        for (key, val) in &props {
            let cf_key = cfstr(key);
            let cf_val = cfnum32(*val);
            unsafe {
                IORegistryEntrySetCFProperty(svc, cf_key, cf_val as CFTypeRef);
                CFRelease(cf_key);
                CFRelease(cf_val as CFTypeRef);
            }
        }
        unsafe { IOObjectRelease(svc) };
    }

    unsafe { IOObjectRelease(iterator) };
    Ok(())
}

/// Find the accelerometer HID device by usage page/usage.
fn find_accel_device() -> Result<IOObject, String> {
    let class_name = CString::new("AppleSPUHIDDevice").unwrap();
    let matching = unsafe { IOServiceMatching(class_name.as_ptr()) };
    if matching.is_null() {
        return Err("failed to create matching dict for AppleSPUHIDDevice".into());
    }

    let mut iterator: IOIterator = 0;
    let kr = unsafe {
        IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching as CFDictionaryRef, &mut iterator)
    };
    if kr != KERN_SUCCESS {
        return Err(format!("IOServiceGetMatchingServices failed for devices: {kr}"));
    }

    loop {
        let svc = unsafe { IOIteratorNext(iterator) };
        if svc == 0 {
            break;
        }

        let usage_page = prop_int(svc, "PrimaryUsagePage");
        let usage = prop_int(svc, "PrimaryUsage");

        if usage_page == Some(PAGE_VENDOR) && usage == Some(USAGE_ACCEL) {
            unsafe { IOObjectRelease(iterator) };
            return Ok(svc);
        }

        unsafe { IOObjectRelease(svc) };
    }

    unsafe { IOObjectRelease(iterator) };
    Err("accelerometer not found — this requires an Apple Silicon MacBook (M2+)".into())
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Start reading the accelerometer. Runs the CFRunLoop on the calling thread
/// (blocks forever). Sends decoded samples through `tx`.
///
/// Must be called from a dedicated thread.
pub fn start(tx: mpsc::Sender<Sample>) -> Result<(), String> {
    wake_spu_drivers()?;
    let accel_service = find_accel_device()?;

    let hid_device = unsafe { IOHIDDeviceCreate(K_CF_ALLOCATOR_DEFAULT, accel_service) };
    if hid_device.is_null() {
        return Err("failed to create IOHIDDevice".into());
    }

    let kr = unsafe { IOHIDDeviceOpen(hid_device, 0) };
    if kr != KERN_SUCCESS {
        return Err(format!(
            "failed to open IOHIDDevice (code {kr}). are you running with sudo?"
        ));
    }

    // These are intentionally leaked — they must live for the lifetime of the
    // callback (i.e. the entire process).
    let report_buf = Box::into_raw(Box::new([0u8; REPORT_BUF_SZ]));
    let ctx = Box::into_raw(Box::new(CallbackContext {
        tx,
        decimation_counter: 0,
    }));

    unsafe {
        IOHIDDeviceRegisterInputReportCallback(
            hid_device,
            report_buf as *mut u8,
            REPORT_BUF_SZ as CFIndex,
            accel_report_callback,
            ctx as *mut c_void,
        );

        let run_loop = CFRunLoopGetCurrent();
        IOHIDDeviceScheduleWithRunLoop(hid_device, run_loop, kCFRunLoopDefaultMode);
        IOObjectRelease(accel_service);
    }

    eprintln!("smack: accelerometer active");

    // Run the CFRunLoop forever (delivers HID reports via callback)
    loop {
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0, false);
        }
    }
}
