use std::sync::mpsc::Sender;

// --- ESTRUCTURA QUE FALTABA DEFINIR ---
#[derive(Clone, Debug)]
pub struct AudioClipData {
    pub path: String,
    pub start_secs: f32,
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
    Seek { sample_count: u64 }, // <--- AGREGAR ESTA LÍNEA
    LoadClip { path: String, position_secs: f32, track_index: usize },
    SyncPlaylistClips { clips: Vec<AudioClipData> },
    ToggleRecord,
    SetBpm(f32),
    TriggerScene { scene_idx: usize },
    TriggerClip { track_idx: usize, scene_idx: usize },
    AddTrack,
    AddScene,
    RemoveScene { scene_idx: usize },
    SetTrackPan { track_idx: usize, pan: f32 },
    SetTrackVolume { track_idx: usize, volume_db: f32 },
    SetMasterVolume { volume_db: f32 },
    SetTrackMute { track_idx: usize, mute: bool },
    SetTrackSolo { track_idx: usize, solo: bool },
    RemoveTrack(usize),
    PreviewSample { 
        path: String, 
        volume: f32, 
        speed: f32, 
    },
    StopPreview,
    SetPreviewVolume(f32),
}