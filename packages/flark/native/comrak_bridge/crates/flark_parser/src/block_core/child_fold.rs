// SPDX-License-Identifier: BSD-2-Clause
// SPDX-FileCopyrightText: 2017-2026 Comrak contributors
// SPDX-FileCopyrightText: 2026 Flark contributors
//
// Mechanically adapted from the Comrak 0.54.0-correspondent child fold in
// `tool/parser_research/comrak_value_block_core/src/tree.rs`. The pinned donor
// commit is 172c2ee7d2c5c262a28be3e407aadf705daea2b7. The complete license
// notice is in `vendor/comrak/COPYING`.

/// The constant-size contribution of one closed child to its parent.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClosedChild {
    ends_blank: bool,
    item_loose_if_nonlast: bool,
    item_loose_if_last: bool,
}

impl ClosedChild {
    #[must_use]
    pub const fn new(
        ends_blank: bool,
        item_loose_if_nonlast: bool,
        item_loose_if_last: bool,
    ) -> Self {
        Self {
            ends_blank,
            item_loose_if_nonlast,
            item_loose_if_last,
        }
    }

    #[must_use]
    pub const fn ends_blank(self) -> bool {
        self.ends_blank
    }

    #[must_use]
    pub const fn item_loose_if_nonlast(self) -> bool {
        self.item_loose_if_nonlast
    }

    #[must_use]
    pub const fn item_loose_if_last(self) -> bool {
        self.item_loose_if_last
    }
}

/// Associative summary of an ordered, already-closed child sequence.
///
/// The last child remains distinct because CommonMark list tightness treats
/// the final item and its final child differently from preceding siblings.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChildSequenceFold {
    had_child: bool,
    any_nonlast_child_ends_blank: bool,
    last_child_ends_blank: bool,
    list_loose_before_last: bool,
    last_item_loose_if_nonlast: bool,
    last_item_loose_if_last: bool,
}

impl ChildSequenceFold {
    #[must_use]
    pub const fn had_child(self) -> bool {
        self.had_child
    }

    #[must_use]
    pub const fn any_nonlast_child_ends_blank(self) -> bool {
        self.any_nonlast_child_ends_blank
    }

    #[must_use]
    pub const fn last_child_ends_blank(self) -> bool {
        self.last_child_ends_blank
    }

    #[must_use]
    pub const fn list_loose_before_last(self) -> bool {
        self.list_loose_before_last
    }

    #[must_use]
    pub const fn last_item_loose_if_nonlast(self) -> bool {
        self.last_item_loose_if_nonlast
    }

    #[must_use]
    pub const fn last_item_loose_if_last(self) -> bool {
        self.last_item_loose_if_last
    }

    /// Adds one child in source order.
    pub fn push(&mut self, child: ClosedChild) {
        if self.had_child {
            self.any_nonlast_child_ends_blank |= self.last_child_ends_blank;
            self.list_loose_before_last |= self.last_item_loose_if_nonlast;
        }
        self.had_child = true;
        self.last_child_ends_blank = child.ends_blank;
        self.last_item_loose_if_nonlast = child.item_loose_if_nonlast;
        self.last_item_loose_if_last = child.item_loose_if_last;
    }

    /// Applies the donor's last-child blank propagation after that child has
    /// already folded out of transient parser scratch.
    pub fn mark_last_child_line_blank(&mut self) {
        if !self.had_child {
            return;
        }
        self.last_child_ends_blank = true;
        self.last_item_loose_if_nonlast = true;
    }

    #[must_use]
    pub const fn list_is_tight(self) -> bool {
        !(self.list_loose_before_last || self.last_item_loose_if_last)
    }

    /// Composes two adjacent child ranges without visiting either range.
    #[must_use]
    pub const fn followed_by(self, suffix: Self) -> Self {
        if !self.had_child {
            return suffix;
        }
        if !suffix.had_child {
            return self;
        }
        Self {
            had_child: true,
            any_nonlast_child_ends_blank: self.any_nonlast_child_ends_blank
                || self.last_child_ends_blank
                || suffix.any_nonlast_child_ends_blank,
            last_child_ends_blank: suffix.last_child_ends_blank,
            list_loose_before_last: self.list_loose_before_last
                || self.last_item_loose_if_nonlast
                || suffix.list_loose_before_last,
            last_item_loose_if_nonlast: suffix.last_item_loose_if_nonlast,
            last_item_loose_if_last: suffix.last_item_loose_if_last,
        }
    }
}
