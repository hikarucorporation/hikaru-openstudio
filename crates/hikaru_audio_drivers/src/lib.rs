/*
 * Hikaru OpenStudio - Native Audio Drivers
 * License: AGPL-3.0-only
 */

pub struct HikaruDriver {
    pub name: String,
}

impl HikaruDriver {
    pub fn new() -> Self {
        Self {
            name: String::from("Hikaru Native ASIO/PipeWire Driver"),
        }
    }
}