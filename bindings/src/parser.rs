use std::cell::Cell;
use std::os::raw::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;
use std::time::Duration;
use std::{fmt, mem, ptr};

use regex_cursor::Cursor;

use crate::syntax_tree::{SyntaxTree, SyntaxTreeData};
use crate::{Grammar, IntoTsInput, Point, Range, TsInput};

// opaque data
enum ParserData {}

#[clippy::msrv = "1.76.0"]
thread_local! {
    static PARSER_CACHE: Cell<Option<RawParser>> = const { Cell::new(None) };
}

struct RawParser {
    ptr: NonNull<ParserData>,
}

impl Drop for RawParser {
    fn drop(&mut self) {
        unsafe { ts_parser_delete(self.ptr) }
    }
}

/// A stateful object that this is used to produce a [`SyntaxTree`] based on some
/// source code.
pub struct Parser {
    ptr: NonNull<ParserData>,
}

impl Parser {
    /// Create a new parser.
    #[must_use]
    pub fn new() -> Parser {
        let ptr = match PARSER_CACHE.take() {
            Some(cached) => {
                let ptr = cached.ptr;
                mem::forget(cached);
                ptr
            }
            None => unsafe { ts_parser_new() },
        };
        Parser { ptr }
    }

    /// Set the language that the parser should use for parsing.
    pub fn set_grammar(&mut self, grammar: Grammar) {
        unsafe { ts_parser_set_language(self.ptr, grammar) };
    }

    pub fn set_timeout(&mut self, duration: Duration) {
        // TODO: migrate away from timing and towards the progress API.
        #[allow(deprecated)]
        unsafe {
            ts_parser_set_timeout_micros(self.ptr, duration.as_micros().try_into().unwrap())
        }
    }

    /// Set the ranges of text that the parser should include when parsing. By default, the parser
    /// will always include entire documents. This function allows you to parse only a *portion*
    /// of a document but still return a syntax tree whose ranges match up with the document as a
    /// whole. You can also pass multiple disjoint ranges.
    ///
    /// `ranges` must be non-overlapping and sorted.
    pub fn set_included_ranges(&mut self, ranges: &[Range]) -> Result<(), InvalidRangesError> {
        // TODO: save some memory by only storing byte ranges and converting them to TS ranges in an
        // internal buffer here. Points are not used by TS. Alternatively we can path the TS C code
        // to accept a simple pair (struct with two fields) of byte positions here instead of a full
        // tree sitter range
        let success = unsafe {
            ts_parser_set_included_ranges(self.ptr, ranges.as_ptr(), ranges.len() as u32)
        };
        if success {
            Ok(())
        } else {
            Err(InvalidRangesError)
        }
    }

    #[must_use]
    pub fn parse<I: TsInput>(
        &mut self,
        input: impl IntoTsInput<TsInput = I>,
        old_tree: Option<&SyntaxTree>,
    ) -> Option<SyntaxTree> {
        let mut input = input.into_ts_input();
        unsafe extern "C" fn read<C: TsInput>(
            payload: NonNull<c_void>,
            byte_index: u32,
            _position: Point,
            bytes_read: *mut u32,
        ) -> *const u8 {
            let cursor = catch_unwind(AssertUnwindSafe(move || {
                let input: &mut C = payload.cast().as_mut();
                let cursor = input.cursor_at(byte_index);
                let slice = cursor.chunk();
                let offset: u32 = cursor.offset().try_into().unwrap();
                let len: u32 = slice.len().try_into().unwrap();
                (byte_index - offset, slice.as_ptr(), len)
            }));
            match cursor {
                Ok((chunk_offset, ptr, len)) if chunk_offset < len => {
                    *bytes_read = len - chunk_offset;
                    ptr.add(chunk_offset as usize)
                }
                _ => {
                    *bytes_read = 0;
                    ptr::null()
                }
            }
        }
        let input = ParserInputRaw {
            payload: NonNull::from(&mut input).cast(),
            read: read::<I>,
            encoding: InputEncoding::Utf8,
            decode: None,
        };
        unsafe {
            let old_tree = old_tree.map(|tree| tree.as_raw());
            let new_tree = ts_parser_parse(self.ptr, old_tree, input);
            new_tree.map(|raw| SyntaxTree::from_raw(raw))
        }
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for Parser {}
unsafe impl Send for Parser {}

impl Drop for Parser {
    fn drop(&mut self) {
        PARSER_CACHE.set(Some(RawParser { ptr: self.ptr }));
    }
}

/// An error that occurred when trying to assign an incompatible [`Grammar`] to
/// a [`Parser`].
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidRangesError;

impl fmt::Display for InvalidRangesError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "include ranges overlap or are not sorted",)
    }
}
impl std::error::Error for InvalidRangesError {}

type TreeSitterReadFn = unsafe extern "C" fn(
    payload: NonNull<c_void>,
    byte_index: u32,
    position: Point,
    bytes_read: *mut u32,
) -> *const u8;

/// A function that reads one code point from the given string, returning the number of bytes
/// consumed.
type DecodeInputFn =
    unsafe extern "C" fn(string: *const u8, length: u32, code_point: *const i32) -> u32;

#[repr(C)]
#[derive(Debug)]
pub struct ParserInputRaw {
    pub payload: NonNull<c_void>,
    pub read: TreeSitterReadFn,
    pub encoding: InputEncoding,
    /// A function to decode the the input.
    ///
    /// This function is only used if the encoding is `InputEncoding::Custom`.
    pub decode: Option<DecodeInputFn>,
}

// `TSInputEncoding`
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum InputEncoding {
    Utf8,
    Utf16LE,
    Utf16BE,
    Custom,
}

extern "C" {
    /// Create a new parser
    fn ts_parser_new() -> NonNull<ParserData>;
    /// Delete the parser, freeing all of the memory that it used.
    fn ts_parser_delete(parser: NonNull<ParserData>);
    /// Set the language that the parser should use for parsing. Returns a boolean indicating
    /// whether or not the language was successfully assigned. True means assignment
    /// succeeded. False means there was a version mismatch: the language was generated with
    /// an incompatible version of the Tree-sitter CLI. Check the language's version using
    /// [`ts_language_version`] and compare it to this library's [`TREE_SITTER_LANGUAGE_VERSION`]
    /// and [`TREE_SITTER_MIN_COMPATIBLE_LANGUAGE_VERSION`] constants.
    fn ts_parser_set_language(parser: NonNull<ParserData>, language: Grammar) -> bool;
    /// Set the ranges of text that the parser should include when parsing. By default, the parser
    /// will always include entire documents. This function allows you to parse only a *portion*
    /// of a document but still return a syntax tree whose ranges match up with the document as a
    /// whole. You can also pass multiple disjoint ranges. The second and third parameters specify
    /// the location and length of an array of ranges. The parser does *not* take ownership of
    /// these ranges; it copies the data, so it doesn't matter how these ranges are allocated.
    /// If `count` is zero, then the entire document will be parsed. Otherwise, the given ranges
    /// must be ordered from earliest to latest in the document, and they must not overlap. That
    /// is, the following must hold for all: `i < count - 1`: `ranges[i].end_byte <= ranges[i +
    /// 1].start_byte` If this requirement is not satisfied, the operation will fail, the ranges
    /// will not be assigned, and this function will return `false`. On success, this function
    /// returns `true`
    fn ts_parser_set_included_ranges(
        parser: NonNull<ParserData>,
        ranges: *const Range,
        count: u32,
    ) -> bool;

    /// Use the parser to parse some source code and create a syntax tree. If you are parsing this
    /// document for the first time, pass `NULL` for the `old_tree` parameter. Otherwise, if you
    /// have already parsed an earlier version of this document and the document has since been
    /// edited, pass the previous syntax tree so that the unchanged parts of it can be reused.
    /// This will save time and memory. For this to work correctly, you must have already edited
    /// the old syntax tree using the [`ts_tree_edit`] function in a way that exactly matches
    /// the source code changes. The [`TSInput`] parameter lets you specify how to read the text.
    /// It has the following three fields: 1. [`read`]: A function to retrieve a chunk of text
    /// at a given byte offset and (row, column) position. The function should return a pointer
    /// to the text and write its length to the [`bytes_read`] pointer. The parser does not
    /// take ownership of this buffer; it just borrows it until it has finished reading it. The
    /// function should write a zero value to the [`bytes_read`] pointer to indicate the end of the
    /// document. 2. [`payload`]: An arbitrary pointer that will be passed to each invocation of
    /// the [`read`] function. 3. [`encoding`]: An indication of how the text is encoded. Either
    /// `TSInputEncodingUTF8` or `TSInputEncodingUTF16`. This function returns a syntax tree
    /// on success, and `NULL` on failure. There are three possible reasons for failure: 1. The
    /// parser does not have a language assigned. Check for this using the [`ts_parser_language`]
    /// function. 2. Parsing was cancelled due to a timeout that was set by an earlier call to the
    /// [`ts_parser_set_timeout_micros`] function. You can resume parsing from where the parser
    /// left out by calling [`ts_parser_parse`] again with the same arguments. Or you can start
    /// parsing from scratch by first calling [`ts_parser_reset`]. 3. Parsing was cancelled using
    /// a cancellation flag that was set by an earlier call to [`ts_parser_set_cancellation_flag`].
    /// You can resume parsing from where the parser left out by calling [`ts_parser_parse`] again
    /// with the same arguments. [`read`]: TSInput::read [`payload`]: TSInput::payload [`encoding`]:
    /// TSInput::encoding [`bytes_read`]: TSInput::read
    fn ts_parser_parse(
        parser: NonNull<ParserData>,
        old_tree: Option<NonNull<SyntaxTreeData>>,
        input: ParserInputRaw,
    ) -> Option<NonNull<SyntaxTreeData>>;
    /// Set the maximum duration in microseconds that parsing should be allowed to
    /// take before halting.
    ///
    /// If parsing takes longer than this, it will halt early, returning NULL.
    /// See [`ts_parser_parse`] for more information.
    #[deprecated = "use ts_parser_parse_with_options and pass in a calback instead, this will be removed in 0.26"]
    fn ts_parser_set_timeout_micros(self_: NonNull<ParserData>, timeout_micros: u64);
}
