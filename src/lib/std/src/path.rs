// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use borrow::{Borrow, IntoCow, ToOwned, Cow};
use cmp;
use iter;
use mem;
use ops::{self, Deref};
use string::String;
use vec::Vec;
use fmt;

use ffi::{OsStr, OsString};

use self::platform::{is_sep_byte, is_verbatim_sep, MAIN_SEP_STR, parse_prefix};

////////////////////////////////////////////////////////////////////////////////
// GENERAL NOTES
////////////////////////////////////////////////////////////////////////////////
//
// Parsing in this module is done by directly transmuting OsStr to [u8] slices,
// taking advantage of the fact that OsStr always encodes ASCII characters
// as-is.  Eventually, this transmutation should be replaced by direct uses of
// OsStr APIs for parsing, but it will take a while for those to become
// available.

////////////////////////////////////////////////////////////////////////////////
// Platform-specific definitions
////////////////////////////////////////////////////////////////////////////////

// The following modules give the most basic tools for parsing paths on various
// platforms. The bulk of the code is devoted to parsing prefixes on Windows.

mod platform {
    use super::Prefix;
    #[cfg(stage0)]
    use core::prelude::v1::*;
    use ffi::OsStr;

    #[inline]
    pub fn is_sep_byte(b: u8) -> bool {
        b == b'/'
    }

    #[inline]
    pub fn is_verbatim_sep(b: u8) -> bool {
        b == b'/'
    }

    pub fn parse_prefix(_: &OsStr) -> Option<Prefix> {
        None
    }

    pub const MAIN_SEP_STR: &'static str = "/";
    pub const MAIN_SEP: char = '/';
}


////////////////////////////////////////////////////////////////////////////////
// Windows Prefixes
////////////////////////////////////////////////////////////////////////////////

/// Path prefixes (Windows only).
///
/// Windows uses a variety of path styles, including references to drive
/// volumes (like `C:`), network shared folders (like `\\server\share`) and
/// others. In addition, some path prefixes are "verbatim", in which case
/// `/` is *not* treated as a separator and essentially no normalization is
/// performed.
#[derive(Copy, Clone, Debug, Hash, PartialOrd, Ord, PartialEq, Eq)]
pub enum Prefix<'a> {
    /// Prefix `\\?\`, together with the given component immediately following it.
    Verbatim(&'a OsStr),

    /// Prefix `\\?\UNC\`, with the "server" and "share" components following it.
    VerbatimUNC(&'a OsStr, &'a OsStr),

    /// Prefix like `\\?\C:\`, for the given drive letter
    VerbatimDisk(u8),

    /// Prefix `\\.\`, together with the given component immediately following it.
    DeviceNS(&'a OsStr),

    /// Prefix `\\server\share`, with the given "server" and "share" components.
    UNC(&'a OsStr, &'a OsStr),

    /// Prefix `C:` for the given disk drive.
    Disk(u8),
}

impl<'a> Prefix<'a> {
    #[inline]
    fn len(&self) -> usize {
        use self::Prefix::*;
        fn os_str_len(s: &OsStr) -> usize {
            os_str_as_u8_slice(s).len()
        }
        match *self {
            Verbatim(x) => 4 + os_str_len(x),
            VerbatimUNC(x,y) => 8 + os_str_len(x) +
                if os_str_len(y) > 0 { 1 + os_str_len(y) }
                else { 0 },
            VerbatimDisk(_) => 6,
            UNC(x,y) => 2 + os_str_len(x) +
                if os_str_len(y) > 0 { 1 + os_str_len(y) }
                else { 0 },
            DeviceNS(x) => 4 + os_str_len(x),
            Disk(_) => 2
        }

    }

    /// Determines if the prefix is verbatim, i.e. begins with `\\?\`.
    #[inline]
    pub fn is_verbatim(&self) -> bool {
        use self::Prefix::*;
        match *self {
            Verbatim(_) | VerbatimDisk(_) | VerbatimUNC(_, _) => true,
            _ => false,
        }
    }

    #[inline]
    fn is_drive(&self) -> bool {
        match *self {
            Prefix::Disk(_) => true,
            _ => false,
        }
    }

    #[inline]
    fn has_implicit_root(&self) -> bool {
        !self.is_drive()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Exposed parsing helpers
////////////////////////////////////////////////////////////////////////////////

/// Determines whether the character is one of the permitted path
/// separators for the current platform.
///
/// # Examples
///
/// ```
/// use std::path;
///
/// assert!(path::is_separator('/'));
/// assert!(!path::is_separator('❤'));
/// ```
pub fn is_separator(c: char) -> bool {
    ((c as u32 ) < 0xff) && is_sep_byte(c as u8)
}

/// The primary separator for the current platform
pub const MAIN_SEPARATOR: char = platform::MAIN_SEP;

////////////////////////////////////////////////////////////////////////////////
// Misc helpers
////////////////////////////////////////////////////////////////////////////////

// Iterate through `iter` while it matches `prefix`; return `None` if `prefix`
// is not a prefix of `iter`, otherwise return `Some(iter_after_prefix)` giving
// `iter` after having exhausted `prefix`.
fn iter_after<A, I, J>(mut iter: I, mut prefix: J) -> Option<I> where
    I: Iterator<Item=A> + Clone, J: Iterator<Item=A>, A: PartialEq
{
    loop {
        let mut iter_next = iter.clone();
        match (iter_next.next(), prefix.next()) {
            (Some(x), Some(y)) => {
                if x != y { return None }
            }
            (Some(_), None) => return Some(iter),
            (None, None) => return Some(iter),
            (None, Some(_)) => return None,
        }
        iter = iter_next;
    }
}

// See note at the top of this module to understand why these are used:
fn os_str_as_u8_slice(s: &OsStr) -> &[u8] {
    unsafe { mem::transmute(s) }
}
unsafe fn u8_slice_as_os_str(s: &[u8]) -> &OsStr {
    mem::transmute(s)
}

////////////////////////////////////////////////////////////////////////////////
// Cross-platform, iterator-independent parsing
////////////////////////////////////////////////////////////////////////////////

/// Says whether the first byte after the prefix is a separator.
fn has_physical_root(s: &[u8], prefix: Option<Prefix>) -> bool {
    let path = if let Some(p) = prefix { &s[p.len()..] } else { s };
    !path.is_empty() && is_sep_byte(path[0])
}

// basic workhorse for splitting stem and extension
#[allow(unused_unsafe)] // FIXME
fn split_file_at_dot(file: &OsStr) -> (Option<&OsStr>, Option<&OsStr>) {
    unsafe {
        if os_str_as_u8_slice(file) == b".." { return (Some(file), None) }

        // The unsafety here stems from converting between &OsStr and &[u8]
        // and back. This is safe to do because (1) we only look at ASCII
        // contents of the encoding and (2) new &OsStr values are produced
        // only from ASCII-bounded slices of existing &OsStr values.

        let mut iter = os_str_as_u8_slice(file).rsplitn(2, |b| *b == b'.');
        let after = iter.next();
        let before = iter.next();
        if before == Some(b"") {
            (Some(file), None)
        } else {
            (before.map(|s| u8_slice_as_os_str(s)),
             after.map(|s| u8_slice_as_os_str(s)))
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// The core iterators
////////////////////////////////////////////////////////////////////////////////

/// Component parsing works by a double-ended state machine; the cursors at the
/// front and back of the path each keep track of what parts of the path have
/// been consumed so far.
///
/// Going front to back, a path is made up of a prefix, a starting
/// directory component, and a body (of normal components)
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
enum State {
    Prefix = 0,         // c:
    StartDir = 1,       // / or . or nothing
    Body = 2,           // foo/bar/baz
    Done = 3,
}

/// A Windows path prefix, e.g. `C:` or `\server\share`.
///
/// Does not occur on Unix.
#[derive(Copy, Clone, Eq, Hash, Debug)]
pub struct PrefixComponent<'a> {
    /// The prefix as an unparsed `OsStr` slice.
    raw: &'a OsStr,

    /// The parsed prefix data.
    parsed: Prefix<'a>,
}

impl<'a> PrefixComponent<'a> {
    /// The parsed prefix data.
    pub fn kind(&self) -> Prefix<'a> {
        self.parsed
    }

    /// The raw `OsStr` slice for this prefix.
    pub fn as_os_str(&self) -> &'a OsStr {
        self.raw
    }
}

impl<'a> cmp::PartialEq for PrefixComponent<'a> {
    fn eq(&self, other: &PrefixComponent<'a>) -> bool {
        cmp::PartialEq::eq(&self.parsed, &other.parsed)
    }
}

impl<'a> cmp::PartialOrd for PrefixComponent<'a> {
    fn partial_cmp(&self, other: &PrefixComponent<'a>) -> Option<cmp::Ordering> {
        cmp::PartialOrd::partial_cmp(&self.parsed, &other.parsed)
    }
}

impl<'a> cmp::Ord for PrefixComponent<'a> {
    fn cmp(&self, other: &PrefixComponent<'a>) -> cmp::Ordering {
        cmp::Ord::cmp(&self.parsed, &other.parsed)
    }
}

/// A single component of a path.
///
/// See the module documentation for an in-depth explanation of components and
/// their role in the API.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Component<'a> {
    /// A Windows path prefix, e.g. `C:` or `\server\share`.
    ///
    /// Does not occur on Unix.
    Prefix(PrefixComponent<'a>),

    /// The root directory component, appears after any prefix and before anything else
    RootDir,

    /// A reference to the current directory, i.e. `.`
    CurDir,

    /// A reference to the parent directory, i.e. `..`
    ParentDir,

    /// A normal component, i.e. `a` and `b` in `a/b`
    Normal(&'a OsStr),
}

impl<'a> Component<'a> {
    /// Extracts the underlying `OsStr` slice
    pub fn as_os_str(self) -> &'a OsStr {
        match self {
            Component::Prefix(p) => p.as_os_str(),
            Component::RootDir => OsStr::new(MAIN_SEP_STR),
            Component::CurDir => OsStr::new("."),
            Component::ParentDir => OsStr::new(".."),
            Component::Normal(path) => path,
        }
    }
}

impl<'a> AsRef<OsStr> for Component<'a> {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

/// The core iterator giving the components of a path.
///
/// See the module documentation for an in-depth explanation of components and
/// their role in the API.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let path = Path::new("/tmp/foo/bar.txt");
///
/// for component in path.components() {
///     println!("{:?}", component);
/// }
/// ```
#[derive(Clone)]
pub struct Components<'a> {
    // The path left to parse components from
    path: &'a [u8],

    // The prefix as it was originally parsed, if any
    prefix: Option<Prefix<'a>>,

    // true if path *physically* has a root separator; for most Windows
    // prefixes, it may have a "logical" rootseparator for the purposes of
    // normalization, e.g.  \\server\share == \\server\share\.
    has_physical_root: bool,

    // The iterator is double-ended, and these two states keep track of what has
    // been produced from either end
    front: State,
    back: State,
}

/// An iterator over the components of a path, as `OsStr` slices.
#[derive(Clone)]
pub struct Iter<'a> {
    inner: Components<'a>
}

impl<'a> Components<'a> {
    // how long is the prefix, if any?
    #[inline]
    fn prefix_len(&self) -> usize {
        self.prefix.as_ref().map(Prefix::len).unwrap_or(0)
    }

    #[inline]
    fn prefix_verbatim(&self) -> bool {
        self.prefix.as_ref().map(Prefix::is_verbatim).unwrap_or(false)
    }

    /// how much of the prefix is left from the point of view of iteration?
    #[inline]
    fn prefix_remaining(&self) -> usize {
        if self.front == State::Prefix { self.prefix_len() }
        else { 0 }
    }

    // Given the iteration so far, how much of the pre-State::Body path is left?
    #[inline]
    fn len_before_body(&self) -> usize {
        let root = if self.front <= State::StartDir && self.has_physical_root { 1 } else { 0 };
        let cur_dir = if self.front <= State::StartDir && self.include_cur_dir() { 1 } else { 0 };
        self.prefix_remaining() + root + cur_dir
    }

    // is the iteration complete?
    #[inline]
    fn finished(&self) -> bool {
        self.front == State::Done || self.back == State::Done || self.front > self.back
    }

    #[inline]
    fn is_sep_byte(&self, b: u8) -> bool {
        if self.prefix_verbatim() {
            is_verbatim_sep(b)
        } else {
            is_sep_byte(b)
        }
    }

    /// Extracts a slice corresponding to the portion of the path remaining for iteration.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path = Path::new("/tmp/foo/bar.txt");
    ///
    /// println!("{:?}", path.components().as_path());
    /// ```
    pub fn as_path(&self) -> &'a Path {
        let mut comps = self.clone();
        if comps.front == State::Body { comps.trim_left(); }
        if comps.back == State::Body { comps.trim_right(); }
        unsafe { Path::from_u8_slice(comps.path) }
    }

    /// Is the *original* path rooted?
    fn has_root(&self) -> bool {
        if self.has_physical_root { return true }
        if let Some(p) = self.prefix {
            if p.has_implicit_root() { return true }
        }
        false
    }

    /// Should the normalized path include a leading . ?
    fn include_cur_dir(&self) -> bool {
        if self.has_root() { return false }
        let mut iter = self.path[self.prefix_len()..].iter();
        match (iter.next(), iter.next()) {
            (Some(&b'.'), None) => true,
            (Some(&b'.'), Some(&b)) => self.is_sep_byte(b),
            _ => false
        }
    }

    // parse a given byte sequence into the corresponding path component
    fn parse_single_component<'b>(&self, comp: &'b [u8]) -> Option<Component<'b>> {
        match comp {
            b"." if self.prefix_verbatim() => Some(Component::CurDir),
            b"." => None, // . components are normalized away, except at
                          // the beginning of a path, which is treated
                          // separately via `include_cur_dir`
            b".." => Some(Component::ParentDir),
            b"" => None,
            _ => Some(Component::Normal(unsafe { u8_slice_as_os_str(comp) }))
        }
    }

    // parse a component from the left, saying how many bytes to consume to
    // remove the component
    fn parse_next_component(&self) -> (usize, Option<Component<'a>>) {
        debug_assert!(self.front == State::Body);
        let (extra, comp) = match self.path.iter().position(|b| self.is_sep_byte(*b)) {
            None => (0, self.path),
            Some(i) => (1, &self.path[.. i]),
        };
        (comp.len() + extra, self.parse_single_component(comp))
    }

    // parse a component from the right, saying how many bytes to consume to
    // remove the component
    fn parse_next_component_back(&self) -> (usize, Option<Component<'a>>) {
        debug_assert!(self.back == State::Body);
        let start = self.len_before_body();
        let (extra, comp) = match self.path[start..].iter().rposition(|b| self.is_sep_byte(*b)) {
            None => (0, &self.path[start ..]),
            Some(i) => (1, &self.path[start + i + 1 ..]),
        };
        (comp.len() + extra, self.parse_single_component(comp))
    }

    // trim away repeated separators (i.e. empty components) on the left
    fn trim_left(&mut self) {
        while !self.path.is_empty() {
            let (size, comp) = self.parse_next_component();
            if comp.is_some() {
                return;
            } else {
                self.path = &self.path[size ..];
            }
        }
    }

    // trim away repeated separators (i.e. empty components) on the right
    fn trim_right(&mut self) {
        while self.path.len() > self.len_before_body() {
            let (size, comp) = self.parse_next_component_back();
            if comp.is_some() {
                return;
            } else {
                self.path = &self.path[.. self.path.len() - size];
            }
        }
    }

    /// Examine the next component without consuming it.
    pub fn peek(&self) -> Option<Component<'a>> {
        self.clone().next()
    }
}

impl<'a> AsRef<Path> for Components<'a> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl<'a> AsRef<OsStr> for Components<'a> {
    fn as_ref(&self) -> &OsStr {
        self.as_path().as_os_str()
    }
}

impl<'a> Iter<'a> {
    /// Extracts a slice corresponding to the portion of the path remaining for iteration.
    pub fn as_path(&self) -> &'a Path {
        self.inner.as_path()
    }
}

impl<'a> AsRef<Path> for Iter<'a> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl<'a> AsRef<OsStr> for Iter<'a> {
    fn as_ref(&self) -> &OsStr {
        self.as_path().as_os_str()
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a OsStr;

    fn next(&mut self) -> Option<&'a OsStr> {
        self.inner.next().map(Component::as_os_str)
    }
}

impl<'a> DoubleEndedIterator for Iter<'a> {
    fn next_back(&mut self) -> Option<&'a OsStr> {
        self.inner.next_back().map(Component::as_os_str)
    }
}

impl<'a> Iterator for Components<'a> {
    type Item = Component<'a>;

    fn next(&mut self) -> Option<Component<'a>> {
        while !self.finished() {
            match self.front {
                State::Prefix if self.prefix_len() > 0 => {
                    self.front = State::StartDir;
                    debug_assert!(self.prefix_len() <= self.path.len());
                    let raw = &self.path[.. self.prefix_len()];
                    self.path = &self.path[self.prefix_len() .. ];
                    return Some(Component::Prefix(PrefixComponent {
                        raw: unsafe { u8_slice_as_os_str(raw) },
                        parsed: self.prefix.unwrap()
                    }))
                }
                State::Prefix => {
                    self.front = State::StartDir;
                }
                State::StartDir => {
                    self.front = State::Body;
                    if self.has_physical_root {
                        debug_assert!(!self.path.is_empty());
                        self.path = &self.path[1..];
                        return Some(Component::RootDir)
                    } else if let Some(p) = self.prefix {
                        if p.has_implicit_root() && !p.is_verbatim() {
                            return Some(Component::RootDir)
                        }
                    } else if self.include_cur_dir() {
                        debug_assert!(!self.path.is_empty());
                        self.path = &self.path[1..];
                        return Some(Component::CurDir)
                    }
                }
                State::Body if !self.path.is_empty() => {
                    let (size, comp) = self.parse_next_component();
                    self.path = &self.path[size ..];
                    if comp.is_some() { return comp }
                }
                State::Body => {
                    self.front = State::Done;
                }
                State::Done => unreachable!()
            }
        }
        None
    }
}

impl<'a> DoubleEndedIterator for Components<'a> {
    fn next_back(&mut self) -> Option<Component<'a>> {
        while !self.finished() {
            match self.back {
                State::Body if self.path.len() > self.len_before_body() => {
                    let (size, comp) = self.parse_next_component_back();
                    self.path = &self.path[.. self.path.len() - size];
                    if comp.is_some() { return comp }
                }
                State::Body => {
                    self.back = State::StartDir;
                }
                State::StartDir => {
                    self.back = State::Prefix;
                    if self.has_physical_root {
                        self.path = &self.path[.. self.path.len() - 1];
                        return Some(Component::RootDir)
                    } else if let Some(p) = self.prefix {
                        if p.has_implicit_root() && !p.is_verbatim() {
                            return Some(Component::RootDir)
                        }
                    } else if self.include_cur_dir() {
                        self.path = &self.path[.. self.path.len() - 1];
                        return Some(Component::CurDir)
                    }
                }
                State::Prefix if self.prefix_len() > 0 => {
                    self.back = State::Done;
                    return Some(Component::Prefix(PrefixComponent {
                        raw: unsafe { u8_slice_as_os_str(self.path) },
                        parsed: self.prefix.unwrap()
                    }))
                }
                State::Prefix => {
                    self.back = State::Done;
                    return None
                }
                State::Done => unreachable!()
            }
        }
        None
    }
}

impl<'a> cmp::PartialEq for Components<'a> {
    fn eq(&self, other: &Components<'a>) -> bool {
        iter::order::eq(self.clone(), other.clone())
    }
}

impl<'a> cmp::Eq for Components<'a> {}

impl<'a> cmp::PartialOrd for Components<'a> {
    fn partial_cmp(&self, other: &Components<'a>) -> Option<cmp::Ordering> {
        iter::order::partial_cmp(self.clone(), other.clone())
    }
}

impl<'a> cmp::Ord for Components<'a> {
    fn cmp(&self, other: &Components<'a>) -> cmp::Ordering {
        iter::order::cmp(self.clone(), other.clone())
    }
}

////////////////////////////////////////////////////////////////////////////////
// Basic types and traits
////////////////////////////////////////////////////////////////////////////////

/// An owned, mutable path (akin to `String`).
///
/// This type provides methods like `push` and `set_extension` that mutate the
/// path in place. It also implements `Deref` to `Path`, meaning that all
/// methods on `Path` slices are available on `PathBuf` values as well.
///
/// More details about the overall approach can be found in
/// the module documentation.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// let mut path = PathBuf::from("c:\\");
/// path.push("windows");
/// path.push("system32");
/// path.set_extension("dll");
/// ```
#[derive(Clone, Hash)]
pub struct PathBuf {
    inner: OsString
}

impl PathBuf {
    fn as_mut_vec(&mut self) -> &mut Vec<u8> {
        unsafe { &mut *(self as *mut PathBuf as *mut Vec<u8>) }
    }

    /// Allocates an empty `PathBuf`.
    pub fn new() -> PathBuf {
        PathBuf { inner: OsString::new() }
    }

    /// Coerces to a `Path` slice.
    pub fn as_path(&self) -> &Path {
        self
    }

    /// Extends `self` with `path`.
    ///
    /// If `path` is absolute, it replaces the current path.
    ///
    /// On Windows:
    ///
    /// * if `path` has a root but no prefix (e.g. `\windows`), it
    ///   replaces everything except for the prefix (if any) of `self`.
    /// * if `path` has a prefix but no root, it replaces `self.
    pub fn push<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();

        // in general, a separator is needed if the rightmost byte is not a separator
        let mut need_sep = self.as_mut_vec().last().map(|c| !is_sep_byte(*c)).unwrap_or(false);

        // in the special case of `C:` on Windows, do *not* add a separator
        {
            let comps = self.components();
            if comps.prefix_len() > 0 &&
                comps.prefix_len() == comps.path.len() &&
                comps.prefix.unwrap().is_drive()
            {
                need_sep = false
            }
        }

        // absolute `path` replaces `self`
        if path.is_absolute() || path.prefix().is_some() {
            self.as_mut_vec().truncate(0);

        // `path` has a root but no prefix, e.g. `\windows` (Windows only)
        } else if path.has_root() {
            let prefix_len = self.components().prefix_remaining();
            self.as_mut_vec().truncate(prefix_len);

        // `path` is a pure relative path
        } else if need_sep {
            self.inner.push(MAIN_SEP_STR);
        }

        self.inner.push(path);
    }

    /// Truncate `self` to `self.parent()`.
    ///
    /// Returns false and does nothing if `self.file_name()` is `None`.
    /// Otherwise, returns `true`.
    pub fn pop(&mut self) -> bool {
        match self.parent().map(|p| p.as_u8_slice().len()) {
            Some(len) => {
                self.as_mut_vec().truncate(len);
                true
            }
            None => false
        }
    }

    /// Updates `self.file_name()` to `file_name`.
    ///
    /// If `self.file_name()` was `None`, this is equivalent to pushing
    /// `file_name`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// let mut buf = PathBuf::from("/");
    /// assert!(buf.file_name() == None);
    /// buf.set_file_name("bar");
    /// assert!(buf == PathBuf::from("/bar"));
    /// assert!(buf.file_name().is_some());
    /// buf.set_file_name("baz.txt");
    /// assert!(buf == PathBuf::from("/baz.txt"));
    /// ```
    pub fn set_file_name<S: AsRef<OsStr>>(&mut self, file_name: S) {
        if self.file_name().is_some() {
            let popped = self.pop();
            debug_assert!(popped);
        }
        self.push(file_name.as_ref());
    }

    /// Updates `self.extension()` to `extension`.
    ///
    /// If `self.file_name()` is `None`, does nothing and returns `false`.
    ///
    /// Otherwise, returns `true`; if `self.extension()` is `None`, the extension
    /// is added; otherwise it is replaced.
    pub fn set_extension<S: AsRef<OsStr>>(&mut self, extension: S) -> bool {
        if self.file_name().is_none() { return false; }

        let mut stem = match self.file_stem() {
            Some(stem) => stem.to_os_string(),
            None => OsString::new(),
        };

        let extension = extension.as_ref();
        if !os_str_as_u8_slice(extension).is_empty() {
            stem.push(".");
            stem.push(extension);
        }
        self.set_file_name(&stem);

        true
    }

    /// Consumes the `PathBuf`, yielding its internal `OsString` storage.
    pub fn into_os_string(self) -> OsString {
        self.inner
    }
}

impl<'a, T: ?Sized + AsRef<OsStr>> From<&'a T> for PathBuf {
    fn from(s: &'a T) -> PathBuf {
        PathBuf::from(s.as_ref().to_os_string())
    }
}

impl From<OsString> for PathBuf {
    fn from(s: OsString) -> PathBuf {
        PathBuf { inner: s }
    }
}

impl From<String> for PathBuf {
    fn from(s: String) -> PathBuf {
        PathBuf::from(OsString::from(s))
    }
}

impl<P: AsRef<Path>> iter::FromIterator<P> for PathBuf {
    fn from_iter<I: IntoIterator<Item = P>>(iter: I) -> PathBuf {
        let mut buf = PathBuf::new();
        buf.extend(iter);
        buf
    }
}

impl<P: AsRef<Path>> iter::Extend<P> for PathBuf {
    fn extend<I: IntoIterator<Item = P>>(&mut self, iter: I) {
        for p in iter {
            self.push(p)
        }
    }
}

impl fmt::Debug for PathBuf {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Debug::fmt(&**self, formatter)
    }
}

impl ops::Deref for PathBuf {
    type Target = Path;

    fn deref(&self) -> &Path {
        Path::new(&self.inner)
    }
}

impl Borrow<Path> for PathBuf {
    fn borrow(&self) -> &Path {
        self.deref()
    }
}

impl IntoCow<'static, Path> for PathBuf {
    fn into_cow(self) -> Cow<'static, Path> {
        Cow::Owned(self)
    }
}

impl<'a> IntoCow<'a, Path> for &'a Path {
    fn into_cow(self) -> Cow<'a, Path> {
        Cow::Borrowed(self)
    }
}

impl ToOwned for Path {
    type Owned = PathBuf;
    fn to_owned(&self) -> PathBuf { self.to_path_buf() }
}

impl cmp::PartialEq for PathBuf {
    fn eq(&self, other: &PathBuf) -> bool {
        self.components() == other.components()
    }
}

impl cmp::Eq for PathBuf {}

impl cmp::PartialOrd for PathBuf {
    fn partial_cmp(&self, other: &PathBuf) -> Option<cmp::Ordering> {
        self.components().partial_cmp(&other.components())
    }
}

impl cmp::Ord for PathBuf {
    fn cmp(&self, other: &PathBuf) -> cmp::Ordering {
        self.components().cmp(&other.components())
    }
}

impl AsRef<OsStr> for PathBuf {
    fn as_ref(&self) -> &OsStr {
        &self.inner[..]
    }
}

impl Into<OsString> for PathBuf {
    fn into(self) -> OsString {
        self.inner
    }
}

/// A slice of a path (akin to `str`).
///
/// This type supports a number of operations for inspecting a path, including
/// breaking the path into its components (separated by `/` or `\`, depending on
/// the platform), extracting the file name, determining whether the path is
/// absolute, and so on. More details about the overall approach can be found in
/// the module documentation.
///
/// This is an *unsized* type, meaning that it must always be used behind a
/// pointer like `&` or `Box`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let path = Path::new("/tmp/foo/bar.txt");
/// let file = path.file_name();
/// let extension = path.extension();
/// let parent_dir = path.parent();
/// ```
///
#[derive(Hash)]
pub struct Path {
    inner: OsStr
}

impl Path {
    // The following (private!) function allows construction of a path from a u8
    // slice, which is only safe when it is known to follow the OsStr encoding.
    unsafe fn from_u8_slice(s: &[u8]) -> &Path {
        Path::new(u8_slice_as_os_str(s))
    }
    // The following (private!) function reveals the byte encoding used for OsStr.
    fn as_u8_slice(&self) -> &[u8] {
        os_str_as_u8_slice(&self.inner)
    }

    /// Directly wrap a string slice as a `Path` slice.
    ///
    /// This is a cost-free conversion.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// Path::new("foo.txt");
    /// ```
    ///
    /// You can create `Path`s from `String`s, or even other `Path`s:
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let string = String::from("foo.txt");
    /// let from_string = Path::new(&string);
    /// let from_path = Path::new(&from_string);
    /// assert_eq!(from_string, from_path);
    /// ```
    pub fn new<S: AsRef<OsStr> + ?Sized>(s: &S) -> &Path {
        unsafe { mem::transmute(s.as_ref()) }
    }

    /// Yields the underlying `OsStr` slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let os_str = Path::new("foo.txt").as_os_str();
    /// assert_eq!(os_str, std::ffi::OsStr::new("foo.txt"));
    /// ```
    pub fn as_os_str(&self) -> &OsStr {
        &self.inner
    }

    /// Yields a `&str` slice if the `Path` is valid unicode.
    ///
    /// This conversion may entail doing a check for UTF-8 validity.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path_str = Path::new("foo.txt").to_str();
    //// assert_eq!(path_str, Some("foo.txt"));
    /// ```
    pub fn to_str(&self) -> Option<&str> {
        self.inner.to_str()
    }

    /// Converts a `Path` to a `Cow<str>`.
    ///
    /// Any non-Unicode sequences are replaced with U+FFFD REPLACEMENT CHARACTER.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path_str = Path::new("foo.txt").to_string_lossy();
    /// assert_eq!(path_str, "foo.txt");
    /// ```
    pub fn to_string_lossy(&self) -> Cow<str> {
        self.inner.to_string_lossy()
    }

    /// Converts a `Path` to an owned `PathBuf`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path_buf = Path::new("foo.txt").to_path_buf();
    /// assert_eq!(path_buf, std::path::PathBuf::from("foo.txt"));
    /// ```
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.inner.to_os_string())
    }

    /// A path is *absolute* if it is independent of the current directory.
    ///
    /// * On Unix, a path is absolute if it starts with the root, so
    /// `is_absolute` and `has_root` are equivalent.
    ///
    /// * On Windows, a path is absolute if it has a prefix and starts with the
    /// root: `c:\windows` is absolute, while `c:temp` and `\temp` are not. In
    /// other words, `path.is_absolute() == path.prefix().is_some() && path.has_root()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// assert!(!Path::new("foo.txt").is_absolute());
    /// ```
    pub fn is_absolute(&self) -> bool {
        self.has_root() &&
            (cfg!(unix) || self.prefix().is_some())
    }

    /// A path is *relative* if it is not absolute.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// assert!(Path::new("foo.txt").is_relative());
    /// ```
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Returns the *prefix* of a path, if any.
    ///
    /// Prefixes are relevant only for Windows paths, and consist of volumes
    /// like `C:`, UNC prefixes like `\\server`, and others described in more
    /// detail in `std::os::windows::PathExt`.
    pub fn prefix(&self) -> Option<Prefix> {
        self.components().prefix
    }

    /// A path has a root if the body of the path begins with the directory separator.
    ///
    /// * On Unix, a path has a root if it begins with `/`.
    ///
    /// * On Windows, a path has a root if it:
    ///     * has no prefix and begins with a separator, e.g. `\\windows`
    ///     * has a prefix followed by a separator, e.g. `c:\windows` but not `c:windows`
    ///     * has any non-disk prefix, e.g. `\\server\share`
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// assert!(Path::new("/etc/passwd").has_root());
    /// ```
    pub fn has_root(&self) -> bool {
         self.components().has_root()
    }

    /// The path without its final component, if any.
    ///
    /// Returns `None` if the path terminates in a root or prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path = Path::new("/foo/bar");
    /// let parent = path.parent().unwrap();
    /// assert_eq!(parent, Path::new("/foo"));
    ///
    /// let grand_parent = parent.parent().unwrap();
    /// assert_eq!(grand_parent, Path::new("/"));
    /// assert_eq!(grand_parent.parent(), None);
    /// ```
    pub fn parent(&self) -> Option<&Path> {
        let mut comps = self.components();
        let comp = comps.next_back();
        comp.and_then(|p| match p {
            Component::Normal(_) |
            Component::CurDir |
            Component::ParentDir => Some(comps.as_path()),
            _ => None
        })
    }

    /// The final component of the path, if it is a normal file.
    ///
    /// If the path terminates in `.`, `..`, or consists solely of a root of
    /// prefix, `file_name` will return `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use std::ffi::OsStr;
    ///
    /// let path = Path::new("foo.txt");
    /// let os_str = OsStr::new("foo.txt");
    ///
    /// assert_eq!(Some(os_str), path.file_name());
    /// ```
    pub fn file_name(&self) -> Option<&OsStr> {
        self.components().next_back().and_then(|p| match p {
            Component::Normal(p) => Some(p.as_ref()),
            _ => None
        })
    }

    /// Returns a path that, when joined onto `base`, yields `self`.
    ///
    /// If `base` is not a prefix of `self` (i.e. `starts_with`
    /// returns false), then `relative_from` returns `None`.
    pub fn relative_from<'a, P: ?Sized + AsRef<Path>>(&'a self, base: &'a P) -> Option<&Path>
    {
        iter_after(self.components(), base.as_ref().components()).map(|c| c.as_path())
    }

    /// Determines whether `base` is a prefix of `self`.
    ///
    /// Only considers whole path components to match.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path = Path::new("/etc/passwd");
    ///
    /// assert!(path.starts_with("/etc"));
    ///
    /// assert!(!path.starts_with("/e"));
    /// ```
    pub fn starts_with<P: AsRef<Path>>(&self, base: P) -> bool {
        iter_after(self.components(), base.as_ref().components()).is_some()
    }

    /// Determines whether `child` is a suffix of `self`.
    ///
    /// Only considers whole path components to match.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path = Path::new("/etc/passwd");
    ///
    /// assert!(path.ends_with("passwd"));
    /// ```
    pub fn ends_with<P: AsRef<Path>>(&self, child: P) -> bool {
        iter_after(self.components().rev(), child.as_ref().components().rev()).is_some()
    }

    /// Extracts the stem (non-extension) portion of `self.file_name()`.
    ///
    /// The stem is:
    ///
    /// * None, if there is no file name;
    /// * The entire file name if there is no embedded `.`;
    /// * The entire file name if the file name begins with `.` and has no other `.`s within;
    /// * Otherwise, the portion of the file name before the final `.`
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path = Path::new("foo.rs");
    ///
    /// assert_eq!("foo", path.file_stem().unwrap());
    /// ```
    pub fn file_stem(&self) -> Option<&OsStr> {
        self.file_name().map(split_file_at_dot).and_then(|(before, after)| before.or(after))
    }

    /// Extracts the extension of `self.file_name()`, if possible.
    ///
    /// The extension is:
    ///
    /// * None, if there is no file name;
    /// * None, if there is no embedded `.`;
    /// * None, if the file name begins with `.` and has no other `.`s within;
    /// * Otherwise, the portion of the file name after the final `.`
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path = Path::new("foo.rs");
    ///
    /// assert_eq!("rs", path.extension().unwrap());
    /// ```
    pub fn extension(&self) -> Option<&OsStr> {
        self.file_name().map(split_file_at_dot).and_then(|(before, after)| before.and(after))
    }

    /// Creates an owned `PathBuf` with `path` adjoined to `self`.
    ///
    /// See `PathBuf::push` for more details on what it means to adjoin a path.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::{Path, PathBuf};
    ///
    /// assert_eq!(Path::new("/etc").join("passwd"), PathBuf::from("/etc/passwd"));
    /// ```
    pub fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.push(path);
        buf
    }

    /// Creates an owned `PathBuf` like `self` but with the given file name.
    ///
    /// See `PathBuf::set_file_name` for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::{Path, PathBuf};
    ///
    /// let path = Path::new("/tmp/foo.txt");
    /// assert_eq!(path.with_file_name("bar.txt"), PathBuf::from("/tmp/bar.txt"));
    /// ```
    pub fn with_file_name<S: AsRef<OsStr>>(&self, file_name: S) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.set_file_name(file_name);
        buf
    }

    /// Creates an owned `PathBuf` like `self` but with the given extension.
    ///
    /// See `PathBuf::set_extension` for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::{Path, PathBuf};
    ///
    /// let path = Path::new("foo.rs");
    /// assert_eq!(path.with_extension("txt"), PathBuf::from("foo.txt"));
    /// ```
    pub fn with_extension<S: AsRef<OsStr>>(&self, extension: S) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.set_extension(extension);
        buf
    }

    /// Produce an iterator over the components of the path.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::{Path, Component};
    /// use std::ffi::OsStr;
    ///
    /// let mut components = Path::new("/tmp/foo.txt").components();
    ///
    /// assert_eq!(components.next(), Some(Component::RootDir));
    /// assert_eq!(components.next(), Some(Component::Normal(OsStr::new("tmp"))));
    /// assert_eq!(components.next(), Some(Component::Normal(OsStr::new("foo.txt"))));
    /// assert_eq!(components.next(), None)
    /// ```
    pub fn components(&self) -> Components {
        let prefix = parse_prefix(self.as_os_str());
        Components {
            path: self.as_u8_slice(),
            prefix: prefix,
            has_physical_root: has_physical_root(self.as_u8_slice(), prefix),
            front: State::Prefix,
            back: State::Body,
        }
    }

    /// Produce an iterator over the path's components viewed as `OsStr` slices.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::{self, Path};
    /// use std::ffi::OsStr;
    ///
    /// let mut it = Path::new("/tmp/foo.txt").iter();
    /// assert_eq!(it.next(), Some(OsStr::new(&path::MAIN_SEPARATOR.to_string())));
    /// assert_eq!(it.next(), Some(OsStr::new("tmp")));
    /// assert_eq!(it.next(), Some(OsStr::new("foo.txt")));
    /// assert_eq!(it.next(), None)
    /// ```
    pub fn iter(&self) -> Iter {
        Iter { inner: self.components() }
    }

    /// Returns an object that implements `Display` for safely printing paths
    /// that may contain non-Unicode data.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let path = Path::new("/tmp/foo.rs");
    ///
    /// println!("{}", path.display());
    /// ```
    pub fn display(&self) -> Display {
        Display { path: self }
    }
}

impl AsRef<OsStr> for Path {
    fn as_ref(&self) -> &OsStr {
        &self.inner
    }
}

impl fmt::Debug for Path {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.inner.fmt(formatter)
    }
}

/// Helper struct for safely printing paths with `format!()` and `{}`
pub struct Display<'a> {
    path: &'a Path
}

impl<'a> fmt::Debug for Display<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.path.to_string_lossy(), f)
    }
}

impl<'a> fmt::Display for Display<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.path.to_string_lossy(), f)
    }
}

impl cmp::PartialEq for Path {
    fn eq(&self, other: &Path) -> bool {
        iter::order::eq(self.components(), other.components())
    }
}

impl cmp::Eq for Path {}

impl cmp::PartialOrd for Path {
    fn partial_cmp(&self, other: &Path) -> Option<cmp::Ordering> {
        self.components().partial_cmp(&other.components())
    }
}

impl cmp::Ord for Path {
    fn cmp(&self, other: &Path) -> cmp::Ordering {
        self.components().cmp(&other.components())
    }
}

impl AsRef<Path> for Path {
    fn as_ref(&self) -> &Path { self }
}

impl AsRef<Path> for OsStr {
    fn as_ref(&self) -> &Path { Path::new(self) }
}

impl AsRef<Path> for OsString {
    fn as_ref(&self) -> &Path { Path::new(self) }
}

impl AsRef<Path> for str {
    fn as_ref(&self) -> &Path { Path::new(self) }
}

impl AsRef<Path> for String {
    fn as_ref(&self) -> &Path { Path::new(self) }
}

impl AsRef<Path> for PathBuf {
    fn as_ref(&self) -> &Path { self }
}

