// Copyright (c) Hikaru Corporation - 2026
// Hikaru OpenStudio - Audio Proxy
// GNU Affero General Public License v3
// crates/hikaru_gui/src/audio_proxy.rs

use std::sync::mpsc::Sender;

#[derive(Clone, Debug)] // HOTFIX CLAUDE #1
pub struct AudioClipData {
    pub clip_id: usize,
    pub path: String,
    pub start_secs: f32,
    pub duration_secs: f32,
    pub offset_secs: f32,
    pub track_index: usize,
}

#[derive(Clone)]
pub struct AudioProxy {
    pub cmd_sender: Sender<GuiCommand>,
}

impl AudioProxy {
    pub fn new(cmd_sender: Sender<GuiCommand>) -> Self {
        Self { cmd_sender }
    }

    pub fn send(&self, cmd: GuiCommand) {
        let _ = self.cmd_sender.send(cmd);
    }
}

pub enum GuiCommand {
    Play,
    Pause,
    Stop,
    Seek { 
        sample_count: u64 
    },
    // ACTUALIZADO: Pasamos el clip_id y sus límites al cargar
    LoadClip { 
        clip_id: usize,
        path: String, 
        position_secs: f32, 
        duration_secs: f32,
        offset_secs: f32,
        track_index: usize 
    },
    UpdateClipBounds { 
        clip_id: usize, 
        position_secs: f32, 
        duration_secs: f32, 
        offset_secs: f32 
    },
    SyncPlaylistClips { 
        clips: Vec<AudioClipData> 
    },
    ToggleRecord,
    SetBpm(f32),
    TriggerScene { 
        scene_idx: usize 
    },
    TriggerClip { 
        track_idx: usize, 
        scene_idx: usize 
    },
    AddTrack,
    AddScene,
    RemoveScene { 
        scene_idx: usize 
    },
    SetTrackPan { 
        track_idx: usize, 
        pan: f32 
    },
    SetTrackVolume { 
        track_idx: usize, 
        volume_db: f32 
    },
    SetMasterVolume { 
        volume_db: f32 
    },
    SetTrackMute { 
        track_idx: usize, 
        mute: bool 
    },
    SetTrackSolo { 
        track_idx: usize, 
        solo: bool 
    },
    RemoveTrack(usize),
    PreviewSample { 
        path: String, 
        volume: f32, 
        speed: f32, 
    },
    StopPreview,
    SetPreviewVolume(f32),
}