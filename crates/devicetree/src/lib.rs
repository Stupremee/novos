#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]

pub mod node;
pub mod parse;

use self::{
    node::Node,
    parse::{Token, TokenIter},
};
use core::{convert::TryInto, ops::Not};

/// The magic number, which is the first 4 bytes in every device tree.
const MAGIC: u32 = 0xD00DFEED;

///  A phandle is a way to reference another node in the devicetree.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PHandle(u32);

impl From<u32> for PHandle {
    fn from(x: u32) -> Self {
        Self(x)
    }
}

impl From<PHandle> for u32 {
    fn from(x: PHandle) -> u32 {
        x.0
    }
}

/// The central structure for working with a flattened device tree.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DeviceTree<'tree> {
    buf: &'tree [u8],
}

impl<'tree> DeviceTree<'tree> {
    /// Tries to create a new `DeviceTree` from a raw pointer to the
    /// flattened device tree.
    ///
    /// # Safety
    ///
    /// - `ptr` must be valid and non-null.
    /// - `ptr` must point to a valid FTD
    /// - `ptr` must not live shorter then the `'tree` lifetime
    ///
    /// # Returns
    ///
    /// `None` if the device tree failed to verify or parse.
    pub unsafe fn from_ptr(ptr: *const u8) -> Option<DeviceTree<'tree>> {
        unsafe fn read_u32(ptr: *const u8) -> u32 {
            let val = *ptr.cast::<u32>();
            u32::from_be(val)
        }

        // read and verify the magic number
        if read_u32(ptr) != MAGIC {
            return None;
        }

        // read `totalsize` to make a slice out of the raw pointer.
        let size = read_u32(ptr.add(4));

        // create the slice and return the tree
        let buf = core::slice::from_raw_parts(ptr, size as usize);
        Some(Self { buf })
    }

    /// Return a raw pointer to the buffer of this device tree.
    pub fn as_ptr(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    /// Consumes this devicetree, copies the raw data of this tree into the buffer,
    /// and returns a reference to the new devicetree.
    pub fn copy_to_slice(self, buf: &mut [u8]) -> DeviceTree<'_> {
        buf[..self.buf.len()].copy_from_slice(self.buf);
        DeviceTree { buf }
    }

    /// Return an iterator over all elemens inside the memory reservations block
    /// of this device tree.
    ///
    /// Memory reservation regions should never be used as normal memory by the
    /// kernel.
    pub fn memory_reservations(&'tree self) -> MemoryReservations<'tree> {
        let start = self.mem_rsv_offset() as usize;
        let data = &self.buf[start..];

        MemoryReservations { data }
    }

    /// Returns the root node of this tree.
    pub fn root(&'tree self) -> Node<'tree> {
        self.find_node("/").expect("there must be a root node")
    }

    /// Returns the memory node of this device tree which
    /// can be used to get all memory regions.
    pub fn memory(&'tree self) -> Node<'tree> {
        self.find_node("/memory")
            .expect("there must be a `/memory` node")
    }

    /// Return the `/cpus` node.
    pub fn cpus(&'tree self) -> Node<'tree> {
        self.find_node("/cpus")
            .expect("there must be a `/cpus` node")
    }

    /// Returns the `/chosen` node of this device tree.
    pub fn chosen(&'tree self) -> node::ChosenNode<'tree> {
        node::ChosenNode {
            tree: self,
            node: self
                .find_node("/chosen")
                .expect("there must be a `/chosen` node"),
        }
    }

    /// Return an iterator over all nodes of this tree.
    pub fn nodes(&'tree self) -> Nodes<'tree> {
        Nodes {
            tree: self,
            iter: self.tokens(),
            level: 0,
        }
    }

    /// Returns an iterator over the raw tokens of the structure block.
    pub fn tokens(&'tree self) -> TokenIter<'tree> {
        let start = self.struct_offset() as usize;
        let size = self.struct_size() as usize;
        let buf = &self.buf[start..start + size];

        TokenIter::new(buf)
    }

    /// Find all nodes that match the given path.
    pub fn find_nodes<'path: 'tree>(
        &'tree self,
        path: &'path str,
    ) -> impl Iterator<Item = Node<'tree>> {
        let mut path = path.split_terminator('/').peekable();
        // nesting_level is the level of the current node
        let mut nesting_level = 0u8;
        // looking_level is the level in which we are currently searching
        let mut looking_level = 1u8;
        let mut iter = self.tokens();

        // the last part of the given path
        let mut last_part = None;

        core::iter::from_fn(move || {
            loop {
                let token = iter.next()?;
                match token {
                    Token::BeginNode(node) => {
                        let level = nesting_level;
                        nesting_level += 1;

                        // we only want to check the nodes that are at the leve we are looking for
                        if nesting_level != looking_level {
                            continue;
                        }

                        // get the next path
                        let next_part = path.peek().copied().or(last_part)?;

                        // otherwise we compare the user provided path
                        // with the current node name
                        if !node.name.starts_with(next_part) {
                            // FIXME: this is a very bad way of checking if two
                            // node names are equal
                            continue;
                        }

                        // if this was the llast part of the part,
                        // we have found our target node
                        path.next();
                        if path.peek().is_none() {
                            last_part = Some(next_part);
                            return Some(Node {
                                tree: self,
                                name: node.name,
                                level,
                                children: iter.clone(),
                            });
                        }

                        // if the names match, continue to the next level
                        looking_level += 1;
                    }
                    Token::EndNode => {
                        nesting_level -= 1;

                        // if the current nesting level is two lower than the looking one,
                        // we haven't found any node
                        if nesting_level < looking_level - 1 {
                            return None;
                        }
                    }
                    // we don't care about properties here
                    Token::Property(_) => {}
                }
            }
        })
    }

    /// Try to find a node at the given path
    pub fn find_node(&'tree self, path: &str) -> Option<Node<'tree>> {
        let mut path = path.split_terminator('/').peekable();
        // nesting_level is the level of the current node
        let mut nesting_level = 0u8;
        // looking_level is the level in which we are currently searching
        let mut looking_level = 1u8;

        let mut iter = self.tokens();
        for token in &mut iter {
            match token {
                Token::BeginNode(node) => {
                    let level = nesting_level;
                    nesting_level += 1;

                    // we only want  to check the nodes that are in the same level
                    if nesting_level != looking_level {
                        continue;
                    }

                    // get the next path
                    let next_part = *path.peek()?;

                    // otherwise we compare the user provided path
                    // with the current node name
                    if !node.name.starts_with(next_part) {
                        // FIXME: this is a very bad way of checking if two
                        // node names are equal
                        continue;
                    }

                    // if this was the llast part of the part,
                    // we have found our target node
                    path.next();
                    if path.peek().is_none() {
                        return Some(Node {
                            tree: self,
                            name: node.name,
                            level,
                            children: iter.clone(),
                        });
                    }

                    // if the names match, continue to the next level
                    looking_level += 1;
                }
                Token::EndNode => {
                    nesting_level -= 1;

                    // if the current nesting level is two lower than the looking one,
                    // we haven't found any node
                    if nesting_level < looking_level - 1 {
                        break;
                    }
                }
                // we don't care about properties here
                Token::Property(_) => {}
            }
        }

        None
    }

    /// Returns the string at the given offset
    ///
    /// # Safety
    ///
    /// The given offset must be a valid offset and point
    /// to a valid string.
    pub unsafe fn string_at(&'tree self, offset: usize) -> Option<&'tree str> {
        let start = self.strings_offset() as usize;
        let buf = self.buf.get(start + offset..)?;
        next_str(buf)
    }

    /// Returns the total size of this device tree structure,
    /// which is read from the header.
    pub fn total_size(&self) -> u32 {
        self.u32_at(1)
    }

    /// Returns the offset of the structure block starting from the
    /// pointer where this device tree begins.
    pub fn struct_offset(&self) -> u32 {
        self.u32_at(2)
    }

    /// Returns the size of the structure block.
    pub fn struct_size(&self) -> u32 {
        self.u32_at(9)
    }

    /// Returns the offset of the strings block starting from the
    /// pointer where this device tree begins.
    pub fn strings_offset(&self) -> u32 {
        self.u32_at(3)
    }

    /// Returns the size of the strings block.
    pub fn strings_size(&self) -> u32 {
        self.u32_at(8)
    }

    /// Returns the offset of the memory reservations block starting from the
    /// pointer where this device tree begins.
    pub fn mem_rsv_offset(&self) -> u32 {
        self.u32_at(4)
    }

    /// Returns the version of this device tree structure.
    pub fn version(&self) -> u32 {
        self.u32_at(5)
    }

    /// Returns the last compatible version of this device tree structure.
    pub fn last_comp_version(&self) -> u32 {
        self.u32_at(6)
    }

    /// Returns the ID of the CPU that boots up the OS.
    pub fn boot_cpu(&self) -> u32 {
        self.u32_at(7)
    }

    /// Return the `idx`nth u32 inside the buffer.
    fn u32_at(&self, idx: usize) -> u32 {
        let real_idx = idx * 4;
        let bytes = &self.buf[real_idx..real_idx + 4];
        u32::from_be_bytes(bytes.try_into().unwrap())
    }
}

pub struct Nodes<'tree> {
    tree: &'tree DeviceTree<'tree>,
    iter: TokenIter<'tree>,
    level: u8,
}

impl<'tree> Iterator for Nodes<'tree> {
    type Item = Node<'tree>;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.iter.next()?;

        match token {
            Token::BeginNode(node) => {
                let level = self.level;
                self.level += 1;

                Some(Node {
                    tree: self.tree,
                    name: node.name,
                    level,
                    children: self.iter.clone(),
                })
            }
            Token::EndNode => {
                self.level -= 1;
                self.next()
            }
            // we don't care about properties here
            Token::Property(_) => self.next(),
        }
    }
}

/// Iterator over all elements inside the memory reservations block.
pub struct MemoryReservations<'tree> {
    data: &'tree [u8],
}

impl Iterator for MemoryReservations<'_> {
    type Item = MemoryReservation;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.data.get(..8)?;
        self.data = &self.data[8..];

        let size = self.data.get(..8)?;
        self.data = &self.data[8..];

        let start = u64::from_be_bytes(start.try_into().ok()?) as usize;
        let size = u64::from_be_bytes(size.try_into().ok()?) as usize;

        // the memory reservations block is terminated by a memory reservation
        // where both fields are 0
        if start == 0 && size == 0 {
            self.data = &[];
            return None;
        }

        Some(MemoryReservation { start, size })
    }
}

/// A region of memory that should not be overwritten because
/// it may contain important data.
pub struct MemoryReservation {
    start: usize,
    size: usize,
}

impl MemoryReservation {
    /// Return the start address of this memory reservation.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Return the end address of this memory reservation.
    pub fn end(&self) -> usize {
        self.start() + self.size()
    }

    /// Return the size of this memory reservation block.
    pub fn size(&self) -> usize {
        self.size
    }
}

/// Search for the first occurrence of `needle` inside `haystack`.
pub fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|&x| x == needle)
}

/// Aligns up the `val` to the given alignment.
pub(crate) fn align_up(val: usize, alignment: usize) -> usize {
    let up = val + (alignment - 1);
    up & !(alignment - 1)
}

pub(crate) unsafe fn next_str(bytes: &[u8]) -> Option<&str> {
    let nul_pos = memchr(0x00, bytes)?;
    let str_bytes = &bytes[..nul_pos];

    Some(core::str::from_utf8_unchecked(str_bytes))
}

pub(crate) fn next_str_checked(bytes: &[u8]) -> Option<&str> {
    let nul_pos = memchr(0x00, bytes)?;
    let str_bytes = &bytes[..nul_pos];

    core::str::from_utf8(str_bytes)
        .ok()
        .and_then(|s| s.is_empty().not().then(|| s))
}
