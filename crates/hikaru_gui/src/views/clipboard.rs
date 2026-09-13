// Copyright (C) Hikaru Corporation - 2026
// Código fuente del Clipboard del OpenLive
// AGPL-v3.0-or-later

use crate::views::matrix::MatrixSlot;

#[derive(Clone, Debug, Default)]
pub struct MatrixClipboard {
    /// Slot copiado o cortado actualmente en el portapapeles.
    pub copied_slot: Option<MatrixSlot>,
}

impl MatrixClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn copy(&mut self, slot: &MatrixSlot) {
        self.copied_slot = Some(slot.clone());
    }

    pub fn has_content(&self) -> bool {
        self.copied_slot.is_some()
    }

    pub fn clear(&mut self) {
        self.copied_slot = None;
    }
}