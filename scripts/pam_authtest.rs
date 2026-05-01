// pam_authtest.rs — minimal pam_authenticate() caller for dev-test.sh.
//
// Single-file program; build with:
//     rustc -O scripts/pam_authtest.rs -l pam -o target/sentinel-authtest
//
// No external crates: just raw FFI against libpam. Equivalent to a tiny
// pamtester(1).

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

#[repr(C)]
struct PamMessage {
    _msg_style: c_int,
    _msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    _resp: *mut c_char,
    _resp_retcode: c_int,
}

#[repr(C)]
struct PamConv {
    conv: unsafe extern "C" fn(
        c_int,
        *const *const PamMessage,
        *mut *mut PamResponse,
        *mut c_void,
    ) -> c_int,
    appdata_ptr: *mut c_void,
}

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service_name: *const c_char,
        user: *const c_char,
        conv: *const PamConv,
        pamh: *mut *mut c_void,
    ) -> c_int;
    fn pam_authenticate(pamh: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut c_void, status: c_int) -> c_int;
    fn pam_strerror(pamh: *mut c_void, errnum: c_int) -> *const c_char;
    fn calloc(nmemb: usize, size: usize) -> *mut c_void;
}

unsafe extern "C" fn conv_fn(
    n: c_int,
    _msgs: *const *const PamMessage,
    resp: *mut *mut PamResponse,
    _data: *mut c_void,
) -> c_int {
    if n <= 0 {
        return PAM_CONV_ERR;
    }
    // pam_sentinel does not prompt; allocate empty responses just in case.
    // libpam frees these via free(), so use calloc to match.
    let buf = unsafe { calloc(n as usize, std::mem::size_of::<PamResponse>()) };
    if buf.is_null() {
        return PAM_BUF_ERR;
    }
    unsafe {
        *resp = buf as *mut PamResponse;
    }
    PAM_SUCCESS
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() != 3 {
        eprintln!("usage: {} SERVICE USER", argv[0]);
        std::process::exit(2);
    }

    let service = CString::new(argv[1].as_str()).expect("service name");
    let user = CString::new(argv[2].as_str()).expect("user name");
    let pc = PamConv {
        conv: conv_fn,
        appdata_ptr: ptr::null_mut(),
    };
    let mut h: *mut c_void = ptr::null_mut();

    let r = unsafe { pam_start(service.as_ptr(), user.as_ptr(), &pc, &mut h) };
    if r != PAM_SUCCESS {
        eprintln!("pam_start: {r}");
        std::process::exit(1);
    }

    let auth = unsafe { pam_authenticate(h, 0) };
    let err = unsafe {
        let p = pam_strerror(h, auth);
        if p.is_null() {
            String::from("unknown")
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    };
    unsafe {
        pam_end(h, auth);
    }

    if auth == PAM_SUCCESS {
        println!("ALLOW");
        std::process::exit(0);
    }
    println!("DENY ({err})");
    std::process::exit(1);
}
