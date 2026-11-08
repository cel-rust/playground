//! The exports `web/cel.js` calls. Strings go in through buffers JavaScript
//! gets from [`alloc`] and frees with [`dealloc`]; a string comes out as a
//! length returned by the call, at [`output_ptr`], valid until the next call.
//!
//! A panic calls the imported `playground.panicked(ptr, len)` with its message
//! before the instance traps: JavaScript then drops that instance.

use std::cell::RefCell;

thread_local! {
    static OUTPUT: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A buffer of `len` bytes for JavaScript to write a string into.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// Frees a buffer from [`alloc`].
///
/// # Safety
///
/// `ptr` and `len` are those of one [`alloc`] call, freed once.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    // SAFETY: `ptr` was allocated by `alloc` as a `Vec<u8>` of capacity `len`,
    // and this is its only free, per this function's contract.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len) });
}

/// Where the string the last call returned the length of starts.
#[no_mangle]
pub extern "C" fn output_ptr() -> *const u8 {
    OUTPUT.with(|output| output.borrow().as_ptr())
}

/// [`crate::evaluate`], on two strings JavaScript wrote into [`alloc`]ed
/// buffers; returns the length of the outcome's JSON, at [`output_ptr`].
///
/// # Safety
///
/// Each pointer and length is a buffer from [`alloc`], initialized up to that
/// length.
#[no_mangle]
pub unsafe extern "C" fn evaluate(
    expression_ptr: *const u8,
    expression_len: usize,
    input_ptr: *const u8,
    input_len: usize,
) -> usize {
    install_panic_hook();
    // SAFETY: per this function's contract.
    let (expression, input) = unsafe {
        (
            text(expression_ptr, expression_len),
            text(input_ptr, input_len),
        )
    };
    output(crate::evaluate(&expression, &input))
}

/// The tokens of the expression JavaScript wrote into an [`alloc`]ed buffer,
/// as `[[start, end, kind], ...]` in UTF-16 code units; returns the length of
/// that JSON, at [`output_ptr`].
///
/// # Safety
///
/// `ptr` and `len` are a buffer from [`alloc`], initialized up to `len`.
#[no_mangle]
pub unsafe extern "C" fn tokens(ptr: *const u8, len: usize) -> usize {
    install_panic_hook();
    // SAFETY: per this function's contract.
    let expression = unsafe { text(ptr, len) };
    output(crate::tokens::to_json(&expression))
}

/// The version of `cel` this build runs; returns its length, at [`output_ptr`].
#[no_mangle]
pub extern "C" fn cel_version() -> usize {
    output(crate::CEL_VERSION.to_owned())
}

/// The playground commit this was built from; returns its length, at
/// [`output_ptr`].
#[no_mangle]
pub extern "C" fn playground_commit() -> usize {
    output(crate::PLAYGROUND_COMMIT.to_owned())
}

/// Panics: `tests/smoke.test.mjs` checks against the module that ships that
/// a panic's message reaches JavaScript, and that the next call works. The
/// page never calls it.
#[no_mangle]
pub extern "C" fn panic_for_tests() -> usize {
    install_panic_hook();
    panic!("panic_for_tests was called");
}

fn output(text: String) -> usize {
    let len = text.len();
    OUTPUT.with(|output| *output.borrow_mut() = text);
    len
}

/// # Safety
///
/// `ptr` is a buffer from [`alloc`], initialized up to `len`.
unsafe fn text(ptr: *const u8, len: usize) -> String {
    // SAFETY: `alloc` returns a non-null, aligned pointer, even for 0 bytes,
    // and the caller initialized `len` bytes from it.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(target_arch = "wasm32")]
fn install_panic_hook() {
    #[link(wasm_import_module = "playground")]
    extern "C" {
        fn panicked(ptr: *const u8, len: usize);
    }

    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let message = info.to_string();
            // SAFETY: JavaScript only reads `len` bytes from `ptr`, during the call.
            unsafe { panicked(message.as_ptr(), message.len()) }
        }))
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn install_panic_hook() {}
