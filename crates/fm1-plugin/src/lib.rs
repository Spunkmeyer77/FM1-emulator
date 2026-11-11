//! The FM-1 emulator as an instrument plugin. One CLAP plugin written with nih-plug (its VST3
//! bindings left out: they are GPL); clap-wrapper makes the AU and the VST3 of it
//! (crates/fm1-plugin/wrap).
//!
//! The firmware is a `.fwsc` package the user loads in the plugin's window: its bytes are saved
//! with the host's project, and so is every flash sector the firmware writes (its patches), so a
//! project reopens with the same firmware and sounds. The host's MIDI goes to the MIDI IN jack
//! (channel messages optionally moved to one channel: M-VAVE's own firmware listens on 2), and
//! the machine's 44.1 kHz output is resampled to the host's rate.

use nih_plug::midi::MidiResult;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use serde::{Deserialize, Serialize};
use std::sync::{mpsc, Arc, Mutex};

mod editor;
pub mod engine;
pub mod savefile;
#[path = "../../fm1-live/src/panel.rs"]
#[allow(dead_code)]
mod panel;
#[path = "../../fm1-live/src/panel_smk.rs"]
#[allow(dead_code)]
mod panel_smk;
mod skin;

use engine::{Cmd, Midi, Shared, View, RATE};

/// The firmware package, saved with the project.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Firmware {
    pub name: String,
    #[serde(with = "b64")]
    pub pkg: Arc<Vec<u8>>,
}

/// A flash sector the firmware wrote, saved with the project.
#[derive(Clone, Serialize, Deserialize)]
pub struct Sector {
    pub at: u32,
    #[serde(with = "b64")]
    pub data: Arc<Vec<u8>>,
}

/// Knob names and their encoder numbers on the FM-1, in the panel's order.
pub const ENC_NAMES: [(&str, u8); 7] =
    [("SELECT", 0), ("PRESETS", 6), ("ALGORITHM", 1), ("KNOB 1", 2), ("KNOB 2", 3), ("KNOB 3", 4), ("KNOB 4", 5)];
/// Clicks for a full sweep of a knob parameter.
pub const ENC_SWEEP: f32 = 128.0;
pub const BTN_NAMES: [&str; 14] =
    ["OCT-", "OCT+", "FX", "SCL", "ENV", "LFO", "EDIT", "GLO", "HOME", "SAVE", "ARP", "SEQ", "PLAY/STOP", "REC"];

#[derive(Params)]
pub struct EncParam {
    #[id = "enc"]
    pub v: FloatParam,
}

#[derive(Params)]
pub struct BtnParam {
    #[id = "btn"]
    pub v: BoolParam,
}

#[derive(Params)]
pub struct Fm1Params {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    #[persist = "firmware"]
    pub firmware: Mutex<Firmware>,
    #[persist = "flash"]
    pub flash: Mutex<Vec<Sector>>,
    /// The panel keys held by incoming MIDI notes (bit i: key i, F3 up), for the editor to show.
    pub held_keys: std::sync::atomic::AtomicU32,

    /// The MASTER pot.
    #[id = "master"]
    pub master: FloatParam,
    /// 0: channel messages as the host sends them; 1..16: moved to that channel.
    #[id = "channel"]
    pub channel: IntParam,
    /// The emulated CPU's clock. Changing it reboots the firmware (its flash is kept).
    #[id = "clock"]
    pub clock: EnumParam<Clock>,
    /// Send MIDI clock and Start/Stop from the host's transport, so the firmware follows its tempo.
    #[id = "sync"]
    pub sync: BoolParam,
    /// The panel's endless knobs as host parameters, for MIDI mapping: a full sweep is 128 clicks (one per MIDI CC step).
    #[nested(array, group = "Knobs")]
    pub encs: [EncParam; 7],
    /// The panel's buttons as host parameters: each change is one press of the button.
    #[nested(array, group = "Buttons")]
    pub btns: [BtnParam; 14],
    /// How far ahead of the host the machine runs beyond one host block: the delay a played
    /// note hears. Changing it changes the latency the plugin reports.
    #[id = "lead"]
    pub lead: EnumParam<Lead>,
}

/// The emulated clock. The emulator runs one instruction a cycle at the chip's nominal 240 MHz,
/// while the AC791N runs some instructions in dual-issue pairs, so a firmware that runs out of
/// time can be given more. A firmware that idles in loops the emulator cannot skip costs host
/// CPU in proportion to the clock, so a lower one lets a heavy firmware keep up on a slower
/// computer (CS-1 and FM-106 run about 1.3x as fast at 160 MHz as at 240, 1.5x at 120), as long
/// as the firmware still has the time it needs.
///
/// v0.2.0 saved the clock as its variant's index, so 240, 320 and 400 keep theirs (0, 1, 2) and
/// the lower clocks come after them; each now saves its id. [`Clock::ALL`] is the menu's order.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clock {
    #[id = "240"]
    #[name = "240 MHz"]
    Mhz240,
    #[id = "320"]
    #[name = "320 MHz"]
    Mhz320,
    #[id = "400"]
    #[name = "400 MHz"]
    Mhz400,
    #[id = "120"]
    #[name = "120 MHz"]
    Mhz120,
    #[id = "160"]
    #[name = "160 MHz"]
    Mhz160,
}

impl Clock {
    pub const ALL: [Clock; 5] = [Clock::Mhz120, Clock::Mhz160, Clock::Mhz240, Clock::Mhz320, Clock::Mhz400];

    pub fn mhz(self) -> u64 {
        match self {
            Clock::Mhz120 => 120,
            Clock::Mhz160 => 160,
            Clock::Mhz240 => 240,
            Clock::Mhz320 => 320,
            Clock::Mhz400 => 400,
        }
    }

    pub fn name(self) -> String {
        format!("{} MHz", self.mhz())
    }
}

/// The machine's lead over the host beyond one host block: room for a DMA half buffer (half the
/// firmwares hand out 256 or 512 frames at once) and for the emulation's uneven pace. A note
/// played live waits this long, plus the host's own buffers, before it sounds; the host makes up
/// the reported latency only for notes it places itself. 1024 frames is the lead the plugin had
/// through 0.4.0; a shorter one shows as "frames late" in the status bar when the emulation
/// falls behind for longer than it. The machine's thread reads it every round; the plugin
/// reports the new latency from `process`, which an AU host does not take while it runs (see
/// the README). Each saves its frames as its id; [`Lead::ALL`] is the menu's order.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lead {
    #[id = "1024"]
    Safe,
    #[id = "512"]
    Low,
    #[id = "256"]
    Lowest,
}

impl Lead {
    pub const ALL: [Lead; 3] = [Lead::Lowest, Lead::Low, Lead::Safe];

    pub fn frames(self) -> usize {
        match self {
            Lead::Safe => 1024,
            Lead::Low => 512,
            Lead::Lowest => 256,
        }
    }

    pub fn name(self) -> String {
        format!("{} ms", (self.frames() as f64 / 44.1).round())
    }
}

impl Default for Fm1Params {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(980, 735),
            firmware: Mutex::new(Firmware::default()),
            flash: Mutex::new(Vec::new()),
            held_keys: std::sync::atomic::AtomicU32::new(0),
            master: FloatParam::new("Master", 900.0 / 1023.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            channel: IntParam::new("MIDI channel", 0, IntRange::Linear { min: 0, max: 16 })
                .with_value_to_string(Arc::new(|v| if v == 0 { "as sent".into() } else { v.to_string() })),
            clock: EnumParam::new("CPU clock", Clock::Mhz240).non_automatable(),
            sync: BoolParam::new("Sync to host", true).non_automatable(),
            encs: std::array::from_fn(|i| EncParam {
                v: FloatParam::new(ENC_NAMES[i].0, 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
            }),
            btns: std::array::from_fn(|i| BtnParam { v: BoolParam::new(BTN_NAMES[i], false) }),
            lead: EnumParam::new("Lead", Lead::Safe).non_automatable(),
        }
    }
}

/// What the plugin's three threads share: the machine's thread runs the firmware, the editor
/// asks it for things, the audio thread takes its frames.
pub struct Link {
    pub shared: Arc<Shared>,
    pub cmds: mpsc::Sender<Cmd>,
    /// the package the machine runs (to boot only when the project's is another)
    pub running: Mutex<Option<Arc<Vec<u8>>>>,
}

impl Link {
    /// Boot `fw` unless the machine already runs it; `sectors` are the project's saved flash.
    pub fn boot(&self, fw: &Firmware, sectors: &[Sector]) {
        let mut running = self.running.lock().unwrap();
        if fw.pkg.is_empty() || running.as_ref().is_some_and(|r| Arc::ptr_eq(r, &fw.pkg) || **r == *fw.pkg) {
            return;
        }
        *running = Some(fw.pkg.clone());
        let sectors = sectors.iter().map(|s| (s.at, s.data.to_vec())).collect();
        let _ = self.cmds.send(Cmd::Boot { pkg: fw.pkg.clone(), sectors });
    }
}

pub struct Fm1 {
    params: Arc<Fm1Params>,
    link: Option<Arc<Link>>,
    thread: Option<std::thread::JoinHandle<()>>,
    midi: Option<rtrb::Producer<Midi>>,
    audio: Option<rtrb::Consumer<[f32; 2]>>,
    /// machine frames per host sample
    ratio: f64,
    /// how far ahead of the host the machine runs, in its frames: a host block and the lead chosen
    lead: usize,
    block: usize,
    lead_of: Lead,
    /// frames taken from the ring, and the resampler between the last two
    taken: u64,
    frac: f64,
    cur: [f32; 2],
    next: [f32; 2],
    /// MIDI clock: whether the host was playing, and the next clock tick (24 a beat) to send
    was_playing: bool,
    next_tick: i64,
    tick_phase: f64,
    /// this block's MIDI (notes and clock), put in time order before it goes to the machine
    pending: Vec<(u64, u32, Midi)>,
}

impl Default for Fm1 {
    fn default() -> Self {
        Self {
            params: Arc::new(Fm1Params::default()),
            link: None,
            thread: None,
            midi: None,
            audio: None,
            ratio: 1.0,
            lead: 2048,
            block: 1024,
            lead_of: Lead::Safe,
            was_playing: false,
            next_tick: 0,
            tick_phase: 0.0,
            pending: Vec::with_capacity(1024),
            taken: 0,
            frac: 0.0,
            cur: [0.0; 2],
            next: [0.0; 2],
        }
    }
}

impl Fm1 {
    /// The machine's thread, once per plugin instance (hosts make instances only to scan them).
    fn start(&mut self) -> Arc<Link> {
        if let Some(l) = &self.link {
            return l.clone();
        }
        let shared = Arc::new(Shared {
            view: Mutex::new(View {
                lcd: vec![0; 240 * 240],
                leds: [0.0; 41],
                pads: [0; 16],
                board: fm1_soc::Board::Fm1,
                booted: false,
                halted: false,
                speed: 0.0,
                starved: 0,
                late: 0,
            }),
            quit: Default::default(),
        });
        let (tx, rx) = mpsc::channel();
        let (midi_tx, midi_rx) = rtrb::RingBuffer::new(4096);
        let (audio_tx, audio_rx) = rtrb::RingBuffer::new(16384);
        let (sh, params) = (shared.clone(), self.params.clone());
        self.thread = Some(
            std::thread::Builder::new()
                .name("fm1-machine".into())
                .spawn(move || engine::run(sh, params, rx, midi_rx, audio_tx))
                .expect("the machine's thread"),
        );
        self.midi = Some(midi_tx);
        self.audio = Some(audio_rx);
        let link = Arc::new(Link { shared, cmds: tx, running: Mutex::new(None) });
        self.link = Some(link.clone());
        link
    }
}

impl Drop for Fm1 {
    fn drop(&mut self) {
        if let Some(l) = &self.link {
            l.shared.quit.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The length of a channel message from its status byte (0: not one).
fn midi_len(status: u8) -> u8 {
    match status & 0xf0 {
        0x80 | 0x90 | 0xa0 | 0xb0 | 0xe0 => 3,
        0xc0 | 0xd0 => 2,
        _ => 0,
    }
}

impl Plugin for Fm1 {
    const NAME: &'static str = "FM-1 emulator";
    const VENDOR: &'static str = "Transistor Labs";
    const URL: &'static str = "https://ticcoross.gumroad.com/";
    const EMAIL: &'static str = "ticcoross@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let link = self.start();
        editor::create(self.params.clone(), link)
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, cfg: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let link = self.start();
        self.ratio = RATE / cfg.sample_rate as f64;
        // a host block's frames, and the lead chosen: room for a DMA half and the emulation's unevenness
        self.block = (cfg.max_buffer_size as f64 * self.ratio).ceil() as usize;
        self.lead_of = self.params.lead.value();
        self.lead = self.block + self.lead_of.frames();
        let _ = link.cmds.send(Cmd::Block(self.block));
        ctx.set_latency_samples((self.lead as f64 / self.ratio).round() as u32);
        let fw = self.params.firmware.lock().unwrap().clone();
        let sectors = self.params.flash.lock().unwrap().clone();
        link.boot(&fw, &sectors);
        true
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        // a new lead: the machine's thread reads the parameter itself; the host hears of the
        // latency here (through nih-plug a restart request, which the AU wrapper does not act on:
        // an AU host keeps the latency it was told until the plugin is loaded again)
        let want = self.params.lead.value();
        if want != self.lead_of {
            self.lead_of = want;
            self.lead = self.block + want.frames();
            ctx.set_latency_samples((self.lead as f64 / self.ratio).round() as u32);
        }
        let start = self.taken;
        let ch = self.params.channel.value() as u8;
        while let Some(ev) = ctx.next_event() {
            let at = ev.timing();
            let Some(MidiResult::Basic(mut b)) = ev.as_midi() else { continue };
            let len = midi_len(b[0]);
            if len == 0 {
                continue;
            }
            // show the note on the panel: its key, folded into the FM-1's F3..G5 by octaves
            let kind = b[0] & 0xf0;
            if kind == 0x80 || kind == 0x90 {
                let mut k = b[1] as i32 - 53;
                while k < 0 { k += 12; }
                while k > 26 { k -= 12; }
                let bit = 1u32 << k;
                let held = &self.params.held_keys;
                if kind == 0x90 && b[2] > 0 {
                    held.fetch_or(bit, std::sync::atomic::Ordering::Relaxed);
                } else {
                    held.fetch_and(!bit, std::sync::atomic::Ordering::Relaxed);
                }
            }
            if ch != 0 {
                b[0] = (b[0] & 0xf0) | (ch - 1);
            }
            let frame = start + (at as f64 * self.ratio) as u64 + self.lead as u64;
            if self.pending.len() < self.pending.capacity() {
                let seq = self.pending.len() as u32;
                self.pending.push((frame, seq, Midi { frame, bytes: b, len }));
            }
        }
        // MIDI clock from the host's transport: Start (or Continue) when it starts, 24 ticks a beat
        // while it plays, Stop when it stops
        {
            let n = buffer.samples();
            let t = ctx.transport();
            let playing = t.playing;
            let host_rate = RATE / self.ratio;
            let base = start + self.lead as u64;
            let ratio = self.ratio;
            let send = |p: &mut Vec<(u64, u32, Midi)>, sample: f64, byte: u8| {
                if p.len() < p.capacity() {
                    let frame = base + (sample.max(0.0) * ratio) as u64;
                    let seq = p.len() as u32;
                    p.push((frame, seq, Midi { frame, bytes: [byte, 0, 0], len: 1 }));
                }
            };
            // the clock runs whenever the host gives a tempo (as a hardware sequencer's clock out
            // does, so a firmware can lock to it before Start); while playing it follows the host's
            // beat position, while stopped (or with no position) it runs on from its own count
            let sync = self.params.sync.value();
            match (sync, t.tempo) {
                (true, Some(tempo)) if tempo > 0.0 => {
                    let ticks_per_sample = tempo / 60.0 * 24.0 / host_rate;
                    let tick_pos = match (playing, t.pos_beats()) {
                        (true, Some(pos)) => {
                            if !self.was_playing {
                                send(&mut self.pending, 0.0, if pos <= 0.001 { 0xfa } else { 0xfb });
                                self.next_tick = (pos * 24.0).ceil() as i64;
                                self.tick_phase = pos * 24.0;
                            }
                            pos * 24.0
                        }
                        _ => self.tick_phase,
                    };
                    if !playing && self.was_playing {
                        send(&mut self.pending, 0.0, 0xfc);
                    }
                    // a jump (loop, locate): carry on from where the host is now
                    if (self.next_tick as f64 - tick_pos).abs() > 2.0 {
                        self.next_tick = tick_pos.ceil() as i64;
                    }
                    let end = tick_pos + n as f64 * ticks_per_sample;
                    while (self.next_tick as f64) < end {
                        let at = (self.next_tick as f64 - tick_pos) / ticks_per_sample;
                        send(&mut self.pending, at, 0xf8);
                        self.next_tick += 1;
                    }
                    self.tick_phase = end;
                    self.was_playing = playing;
                }
                _ => {
                    if self.was_playing {
                        send(&mut self.pending, 0.0, 0xfc);
                    }
                    self.was_playing = false;
                }
            }
        }
        self.pending.sort_unstable_by_key(|e| (e.0, e.1));
        if let Some(m) = self.midi.as_mut() {
            for (_, _, e) in self.pending.drain(..) {
                let _ = m.push(e);
            }
        } else {
            self.pending.clear();
        }
        let Some(rx) = self.audio.as_mut() else { return ProcessStatus::Normal };
        let out = buffer.as_slice();
        let n = out[0].len();
        let mut starved = 0u64;
        for i in 0..n {
            self.frac += self.ratio;
            while self.frac >= 1.0 {
                match rx.pop() {
                    Ok(f) => {
                        self.cur = self.next;
                        self.next = f;
                        self.taken += 1;
                        self.frac -= 1.0;
                    }
                    Err(_) => {
                        // the machine is behind: hold, and take the frame next time
                        self.frac = self.frac.min(1.0);
                        starved += 1;
                        break;
                    }
                }
            }
            let t = self.frac.min(1.0) as f32;
            for c in 0..2 {
                out[c][i] = self.cur[c] + (self.next[c] - self.cur[c]) * t;
            }
        }
        if starved > 0 {
            if let Some(l) = &self.link {
                if let Ok(mut v) = l.shared.view.try_lock() {
                    v.starved += starved;
                }
            }
        }
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for Fm1 {
    const CLAP_ID: &'static str = "com.transistorlabs.fm1-emulator";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("FM-1 emulator by Transistor Labs: the M-VAVE FM-1 pocket synth emulated (unofficial)");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Instrument, ClapFeature::Synthesizer, ClapFeature::Stereo];
}

nih_export_clap!(Fm1);

/// Bytes as base64 in the saved state (a package is ~600 KB).
mod b64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::sync::Arc;

    pub fn serialize<S: Serializer>(v: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v.as_slice()))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(s).map(Arc::new).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_2_0_projects_keep_their_clock() {
        // v0.2.0 saved the clock as its index: 0, 1, 2 for 240, 320, 400
        assert_eq!([0, 1, 2].map(|i| Clock::from_index(i).mhz()), [240, 320, 400]);
        // the menu goes up the clocks, and each saves an id of its own
        assert!(Clock::ALL.windows(2).all(|w| w[0].mhz() < w[1].mhz()));
        let ids = Clock::ids().unwrap();
        assert!(Clock::ALL.iter().all(|c| ids[c.to_index()] == c.mhz().to_string()));
    }

    #[test]
    fn leads_save_their_frames_and_the_menu_goes_up() {
        let ids = Lead::ids().unwrap();
        assert_eq!(Lead::ALL.len(), Lead::variants().len());
        assert!(Lead::ALL.iter().all(|l| ids[l.to_index()] == l.frames().to_string()));
        assert!(Lead::ALL.windows(2).all(|w| w[0].frames() < w[1].frames()));
        assert_eq!(Lead::Safe.frames(), 1024, "the lead the plugin had through 0.4.0");
        assert_eq!(Lead::ALL.map(Lead::name), ["6 ms", "12 ms", "23 ms"]);
    }
}
