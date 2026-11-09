use engine::game::CollisionEvent;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::PathBuf;

const CONTACTO_NUEVO: f32 = 0.5;
const ENFRIAMIENTO: f32 = 0.15;
const VOLUMEN_MUSICA: f32 = 0.15;
const VOLUMEN_MOTOR: f32 = 0.5;
const FACTOR_EXPLOSION: f32 = 0.5;
const VOLUMEN_PUNTO: f32 = 0.8;

pub struct Sonidos {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    explosion: Option<Vec<u8>>,
    punto: Option<Vec<u8>>,
    agujero: Option<Vec<u8>>,
    motor: Option<Vec<u8>>,
    sink_motor: Option<Sink>,
    _musica: Option<Sink>,
    activos: Vec<Sink>,
    reloj: f32,
    ultimo_choque: f32,
    /// Último momento en que se vio un choque con el agujero negro (la gravedad lo repite sin parar).
    ultimo_agujero: f32,
}

impl Sonidos {
    pub fn new() -> Option<Self> {
        let (_stream, handle) = OutputStream::try_default().ok()?;
        let mut carpeta = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        carpeta.pop();
        carpeta.push("assets");
        carpeta.push("sound");

        let musica = (|| {
            let fuente = Decoder::new(BufReader::new(File::open(carpeta.join("background.ogg")).ok()?)).ok()?;
            let sink = Sink::try_new(&handle).ok()?;
            sink.set_volume(VOLUMEN_MUSICA);
            sink.append(fuente.repeat_infinite());
            Some(sink)
        })();
        let leer = |n: &str| std::fs::read(carpeta.join(n)).ok();

        Some(Self {
            _stream,
            handle,
            explosion: leer("explosion.ogg"),
            punto: leer("point.ogg"),
            agujero: leer("blackhole.ogg"),
            motor: leer("motor.ogg"),
            sink_motor: None,
            _musica: musica,
            activos: Vec::new(),
            reloj: 0.0,
            ultimo_choque: f32::NEG_INFINITY,
            ultimo_agujero: f32::NEG_INFINITY,
        })
    }

    /// `avanzando`: W presionada en la pantalla de juego.
    pub fn actualizar(&mut self, dt: f32, avanzando: bool, choques: &[CollisionEvent], aros: u32) {
        self.reloj += dt;
        self.activos.retain(|s| !s.empty());

        if aros > 0 {
            if let Some(sink) = self.punto.clone().and_then(|b| self.tocar(b, VOLUMEN_PUNTO)) {
                self.activos.push(sink);
            }
        }

        // Un disparo al presionar W; al terminar, si sigue presionada, se repite.
        // Al soltar W se corta.
        if avanzando {
            if self.sink_motor.as_ref().is_none_or(|s| s.empty()) {
                self.sink_motor = self.motor.clone().and_then(|b| self.tocar(b, VOLUMEN_MOTOR));
            }
        } else if let Some(s) = self.sink_motor.take() {
            s.stop();
        }

        for c in choques {
            if c.body == "AGUJERO NEGRO" {
                // Un solo sonido por contacto: mientras siga pegado no se repite.
                let nuevo = self.reloj - self.ultimo_agujero > CONTACTO_NUEVO;
                self.ultimo_agujero = self.reloj;
                if !nuevo {
                    continue;
                }
            } else if self.reloj - self.ultimo_choque < ENFRIAMIENTO {
                continue;
            }
            let factor = if c.body == "AGUJERO NEGRO" { 1.0 } else { FACTOR_EXPLOSION };
            let bytes = if c.body == "AGUJERO NEGRO" { &self.agujero } else { &self.explosion };
            if let Some(sink) = bytes.clone().and_then(|b| self.tocar(b, (c.impact_speed / 30.0).clamp(0.3, 1.0) * factor)) {
                self.ultimo_choque = self.reloj;
                self.activos.push(sink);
            }
        }
    }

    fn tocar(&self, bytes: Vec<u8>, volumen: f32) -> Option<Sink> {
        let fuente = Decoder::new(Cursor::new(bytes)).ok()?;
        let sink = Sink::try_new(&self.handle).ok()?;
        sink.set_volume(volumen);
        sink.append(fuente);
        Some(sink)
    }
}
