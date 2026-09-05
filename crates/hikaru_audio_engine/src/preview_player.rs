// crates/hikaru_audio_engine/src/preview_player.rs
// Reproductor de vista previa de muestras — motor de audio en tiempo real.
// Cero bloqueos, cero alocaciones en el callback de CPAL.

/// Capacidad máxima de muestras mono para preview (aprox. 5 min @ 48kHz).
/// Se usa para pre-alocar el buffer interno y evitar realocaciones dinámicas
/// durante la reproducción.
const MAX_PREVIEW_SAMPLES: usize = 14_400_000;

/// Estado del reproductor de preview.
///
/// Este struct vive dentro del callback de audio de CPAL. Por eso:
/// - No usa Mutex/RwLock.
/// - No aloca memoria en `process()`.
/// - El buffer se reemplaza entero en `play()` (evento de control, no callback).
pub struct PreviewPlayer {
    /// Buffer interno de muestras mono f32 pre-alocadas.
    /// Se trunca o reemplaza en `play()`, nunca se redimensiona en `process()`.
    buffer: Vec<f32>,
    /// Índice de la muestra actual en reproducción.
    position: usize,
    /// Factor de ganancia lineal: 0.0 = silencio absoluto, 1.0 = volumen máximo.
    volume: f32,
    /// Flag de reproducción activa. Se levanta en `play()` y baja en `stop()` o al finalizar el buffer.
    is_playing: bool,
}

impl PreviewPlayer {
    /// Crea un `PreviewPlayer` con buffer pre-alocado y volumen por defecto al 80%.
    pub fn new() -> Self {
        let mut buffer = Vec::with_capacity(MAX_PREVIEW_SAMPLES);
        buffer.resize(MAX_PREVIEW_SAMPLES, 0.0);

        Self {
            buffer,
            position: 0,
            volume: 0.8,
            is_playing: false,
        }
    }

    /// Establece el volumen de reproducción.
    ///
    /// Rango válido: `0.0` (mute absoluto) a `1.0` (máximo).
    /// Valores fuera de rango se clampan.
    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);
    }

    /// Carga nuevas muestras y comienza la reproducción desde el principio.
    ///
    /// Si `samples` excede `MAX_PREVIEW_SAMPLES`, se trunca silenciosamente
    /// para respetar el límite pre-alocado.
    pub fn play(&mut self, samples: Vec<f32>) {
        let len = samples.len().min(MAX_PREVIEW_SAMPLES);

        // Copiamos las muestras al buffer pre-alocado. No hay realocación.
        self.buffer[..len].copy_from_slice(&samples[..len]);

        // Si el buffer original era más largo, limpiamos el resto para evitar
        // ruido residual en reproducciones futuras más cortas.
        if len < MAX_PREVIEW_SAMPLES {
            self.buffer[len..].fill(0.0);
        }

        self.position = 0;
        self.is_playing = true;
    }

    /// Detiene la reproducción, resetea la posición a cero y limpia el estado.
    ///
    /// El buffer interno se mantiene pre-alocado; solo se limpia su contenido
    /// para evitar que queden muestras "fantasma" si se reanuda sin `play()`.
    pub fn stop(&mut self) {
        self.is_playing = false;
        self.position = 0;
        self.buffer.fill(0.0);
    }

    /// Callback de procesamiento de audio — ejecutado en el hilo de CPAL.
    ///
    /// Mezcla las muestras del buffer en `output` aplicando el volumen actual.
    /// Cuando el buffer se agota, levanta `is_playing = false` automáticamente.
    ///
    /// # Lock-free / No-alloc garantía
    /// - Solo lectura/escritura de campos primitivos.
    /// - Sin `Vec::push`, `Box`, `String`, `Mutex` ni `RwLock`.
    pub fn process(&mut self, output: &mut [f32]) {
        if !self.is_playing {
            return;
        }

        let buf_len = self.buffer.len();
        let vol = self.volume;

        for frame in output.iter_mut() {
            if self.position < buf_len {
                // Mezcla aditiva: sumamos al frame existente para permitir
                // superposición con otras fuentes en el motor maestro.
                *frame += self.buffer[self.position] * vol;
                self.position += 1;
            } else {
                // Fin del buffer: frenamos limpio.
                self.is_playing = false;
                break;
            }
        }
    }
}

// ============================================================================
// Tests unitarios
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_clamps_to_zero() {
        let mut player = PreviewPlayer::new();
        player.set_volume(0.0);
        assert_eq!(player.volume, 0.0);
    }

    #[test]
    fn test_volume_clamps_to_one() {
        let mut player = PreviewPlayer::new();
        player.set_volume(1.5);
        assert_eq!(player.volume, 1.0);
    }

    #[test]
    fn test_stop_resets_state() {
        let mut player = PreviewPlayer::new();
        player.play(vec![0.5; 100]);
        player.stop();

        assert!(!player.is_playing);
        assert_eq!(player.position, 0);
        assert!(player.buffer.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_process_mutes_at_zero_volume() {
        let mut player = PreviewPlayer::new();
        player.play(vec![1.0; 10]);
        player.set_volume(0.0);

        let mut output = vec![0.0; 10];
        player.process(&mut output);

        assert!(output.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_process_stops_at_buffer_end() {
        let mut player = PreviewPlayer::new();
        player.play(vec![0.5; 5]);

        let mut output = vec![0.0; 10];
        player.process(&mut output);

        assert!(!player.is_playing);
        assert_eq!(player.position, 5);
        assert_eq!(output[..5], [0.4; 5]); // 0.5 * 0.8 (volumen por defecto)
        assert_eq!(output[5..], [0.0; 5]);
    }
}