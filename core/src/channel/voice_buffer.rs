use super::ChannelInitOptions;
use crate::{
    effects::{FilterId, SIMDBiQuadState, SharedFilter},
    helpers::sum_simd,
    voice::{ReleaseType, Voice, VoiceFilter},
};
use std::{
    collections::VecDeque,
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::Arc,
};

struct GroupVoice {
    pub id: usize,
    pub voice: Box<dyn Voice>,
    /// Set by `render_voices` when the voice finished playing
    pub ended: bool,
    /// Index of the filter bus used last, checked before searching the buses
    pub bus: u8,
    /// The shared filter of the voice, cached to skip a dynamic call per block
    pub filter: Option<Arc<VoiceFilter>>,
}

impl Deref for GroupVoice {
    type Target = Box<dyn Voice>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.voice
    }
}

impl DerefMut for GroupVoice {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Box<dyn Voice> {
        &mut self.voice
    }
}

impl Debug for GroupVoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("")
            .field(&self.id)
            .field(&self.voice.velocity())
            .field(&self.voice.is_killed())
            .finish()
    }
}

pub struct VoiceBuffer {
    options: ChannelInitOptions,
    id_counter: usize,
    buffer: VecDeque<GroupVoice>,
    damper_held: bool,
    held_by_damper: Vec<usize>,
    /// Voice groups captured by the sostenuto pedal
    sostenuto_ids: Vec<usize>,
    /// Voice groups that were released while held by the sostenuto pedal
    held_by_sostenuto: Vec<usize>,
}

impl VoiceBuffer {
    pub fn new(options: ChannelInitOptions) -> Self {
        VoiceBuffer {
            options,
            id_counter: 0,
            buffer: VecDeque::new(),
            damper_held: false,
            held_by_damper: Vec::new(),
            sostenuto_ids: Vec::new(),
            held_by_sostenuto: Vec::new(),
        }
    }

    fn get_id(&mut self) -> usize {
        self.id_counter += 1;
        self.id_counter
    }

    /// Pops the quietest voice group. Multiple voices can be part of the same group
    /// based on their ID (e.g. a note and a hammer playing at the same time for a note on event)
    fn pop_quietest_voice_group(&mut self, ignored_id: usize) {
        if self.buffer.is_empty() {
            return;
        }

        let mut quietest = u8::MAX;
        let mut quietest_index = 0;
        let mut quietest_id = 0;
        let mut count = 0;
        for (i, voice) in self.buffer.iter().enumerate() {
            if voice.id == ignored_id || voice.is_killed() {
                continue;
            }
            let vel = voice.velocity();
            if quietest_id == voice.id {
                count += 1;
            } else if vel < quietest || i == 0 {
                quietest = vel;
                quietest_index = i;
                quietest_id = voice.id;
                count = 1;
            }
        }

        if count > 0 {
            if self.options.fade_out_killing {
                for i in quietest_index..(quietest_index + count) {
                    self.kill_voice_fade_out(i);
                }
            } else {
                self.buffer.drain(quietest_index..(quietest_index + count));
            }

            if let Some(index) = self.held_by_damper.iter().position(|&x| x == quietest_id) {
                self.held_by_damper.remove(index);
            }
        }
    }

    fn kill_voice_fade_out(&mut self, index: usize) {
        self.buffer[index]
            .deref_mut()
            .signal_release(ReleaseType::Kill);
    }

    pub fn kill_all_voices(&mut self) {
        self.sostenuto_ids.clear();
        self.held_by_sostenuto.clear();
        if self.options.fade_out_killing {
            for i in 0..self.buffer.len() {
                self.kill_voice_fade_out(i);
            }
            self.id_counter = 0;
        } else {
            self.buffer.clear();
        }
    }

    pub fn kill_by_exclusive_class(&mut self, class: u8) {
        for voice in &mut self.buffer {
            if voice.exclusive_class() == Some(class) {
                voice.signal_release(ReleaseType::Kill);
            }
        }
    }

    fn get_active_count(&mut self) -> usize {
        self.buffer
            .iter()
            .filter(|voice| !voice.voice.is_killed())
            .count()
    }

    /// Pushes a new set of voices for a single note on event. Multiple voices can be part of the same group
    /// based on their ID (e.g. a note and a hammer playing at the same time for a note on event)
    pub fn push_voices(
        &mut self,
        voices: impl Iterator<Item = Box<dyn Voice>>,
        max_voices: Option<usize>,
    ) {
        let mut len = 0;

        let id = self.get_id();
        for voice in voices {
            let filter = voice.shared_filter().cloned();
            self.buffer.push_back(GroupVoice {
                id,
                voice,
                ended: false,
                bus: 0,
                filter,
            });
            len += 1;
        }

        if let Some(max_voices) = max_voices {
            if len > max_voices {
                self.pop_quietest_voice_group(id);
            } else if self.options.fade_out_killing {
                while self.get_active_count() > max_voices {
                    self.pop_quietest_voice_group(id);
                }
            } else {
                while self.buffer.len() > max_voices {
                    self.pop_quietest_voice_group(id);
                }
            }
        }
    }

    /// Releases the next voice, and all subsequent voices that have the same ID.
    pub fn release_next_voice(&mut self) -> Option<u8> {
        if !self.sostenuto_ids.is_empty() && !self.damper_held {
            // Voices captured by the sostenuto pedal keep playing until it is lifted.
            for voice in self.buffer.iter() {
                if voice.is_releasing() || self.held_by_sostenuto.contains(&voice.id) {
                    continue;
                }
                if self.sostenuto_ids.contains(&voice.id) {
                    self.held_by_sostenuto.push(voice.id);
                    return None;
                }
                break;
            }
        }

        if !self.damper_held {
            let mut id: Option<usize> = None;
            let mut vel = None;

            // Find the first non releasing voice, get its id and release all voices with that id
            for voice in self.buffer.iter_mut() {
                if voice.is_releasing() || self.held_by_sostenuto.contains(&voice.id) {
                    continue;
                }

                if id.is_none() {
                    id = Some(voice.id);
                    vel = Some(voice.velocity())
                }

                if id != Some(voice.id) {
                    break;
                }

                voice.signal_release(ReleaseType::Standard);
            }

            vel
        } else {
            // Find the first non releasing voice which also isn't being held in the release buffer, and add it to the release buffer
            for voice in self.buffer.iter_mut() {
                if voice.is_releasing() {
                    continue;
                }

                if self.held_by_damper.contains(&voice.id)
                    || self.held_by_sostenuto.contains(&voice.id)
                {
                    continue;
                }

                self.held_by_damper.push(voice.id);
                break;
            }

            None
        }
    }

    // pub fn iter_voices<'a>(&'a self) -> impl Iterator<Item = &Box<dyn Voice>> + 'a {
    //     self.buffer.iter().map(|group| &group.voice)
    // }

    pub fn iter_voices_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Voice>> {
        self.buffer.iter_mut().map(|group| &mut group.voice)
    }

    /// Renders every voice into `out`. The state of each voice is prefetched
    /// while the previous one renders: with many voices and short buffers, the
    /// voice states do not stay in cache between buffers.
    ///
    /// Voices that ended are removed afterwards, checked right after rendering
    /// while their state is still in cache.
    ///
    /// Voices with a shared filter render into the matching bus of `buses`,
    /// which is filtered once and added to `out` afterwards.
    pub fn render_voices(&mut self, out: &mut [f32], buses: &mut FilterBuses) {
        let mut any_ended = false;
        let frames = out.len() / buses.channels;
        buses.begin(frames);
        for ahead in self.buffer.iter().take(STATE_PREFETCH_DISTANCE).skip(1) {
            prefetch_voice(&*ahead.voice);
        }
        for i in 0..self.buffer.len() {
            // Voice states are loaded STATE_PREFETCH_DISTANCE voices ahead,
            // then the sample data of a voice whose state is in cache by now.
            if let Some(ahead) = self.buffer.get(i + STATE_PREFETCH_DISTANCE) {
                prefetch_voice(&*ahead.voice);
            }
            if let Some(next) = self.buffer.get(i + SAMPLE_PREFETCH_DISTANCE) {
                next.voice.prefetch(frames);
            }
            let voice = &mut self.buffer[i];
            let (left, right) = match &voice.filter {
                Some(filter) => buses.bus_for(filter, &mut voice.bus, frames),
                None => buses.dry(frames),
            };
            voice.ended = voice.voice.render_block(left, right);
            any_ended |= voice.ended;
        }
        buses.finish(out);
        if any_ended {
            self.buffer.retain(|voice| !voice.ended);
        }
    }

    pub fn has_voices(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn voice_count(&self) -> usize {
        self.buffer.len()
    }

    pub fn set_damper(&mut self, damper: bool) {
        if self.damper_held && !damper {
            // Release all voices that are held by the damper, except the ones
            // the sostenuto pedal still holds
            for id in self.held_by_damper.drain(..) {
                if self.sostenuto_ids.contains(&id) {
                    self.held_by_sostenuto.push(id);
                    continue;
                }
                for voice in self.buffer.iter_mut() {
                    if voice.id == id {
                        voice.signal_release(ReleaseType::Standard);
                    }
                }
            }
        }
        self.damper_held = damper;
    }

    pub fn set_sostenuto(&mut self, sostenuto: bool) {
        if sostenuto {
            // Capture the voice groups that are currently held down
            self.sostenuto_ids.clear();
            for voice in self.buffer.iter() {
                if !voice.is_releasing()
                    && !self.held_by_damper.contains(&voice.id)
                    && !self.sostenuto_ids.contains(&voice.id)
                {
                    self.sostenuto_ids.push(voice.id);
                }
            }
        } else {
            self.sostenuto_ids.clear();
            for id in self.held_by_sostenuto.drain(..) {
                if self.damper_held {
                    self.held_by_damper.push(id);
                    continue;
                }
                for voice in self.buffer.iter_mut() {
                    if voice.id == id {
                        voice.signal_release(ReleaseType::Standard);
                    }
                }
            }
        }
    }
}

/// How many voices ahead the voice states and their sample data are
/// prefetched. Rendering one voice for a short buffer takes about as long as a
/// memory access, so both reach a few voices ahead.
const STATE_PREFETCH_DISTANCE: usize = 8;
const SAMPLE_PREFETCH_DISTANCE: usize = 4;

/// Starts loading the state of a voice into cache.
#[inline(always)]
fn prefetch_voice(voice: &dyn Voice) {
    #[cfg(target_arch = "x86_64")]
    {
        use core::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        // A fixed size avoids reading the vtable; sample voices are cache line
        // aligned and fit in three lines.
        let start = voice as *const dyn Voice as *const u8;
        let mut offset = 0;
        while offset < 192 {
            // Safety: prefetching never faults.
            unsafe { _mm_prefetch::<_MM_HINT_T0>(start.wrapping_add(offset) as *const i8) };
            offset += 64;
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = voice;
}

struct FilterBus {
    /// The handle of the voices that first used the bus, compared by pointer
    /// so that most lookups do not read it
    source: Arc<VoiceFilter>,
    id: FilterId,
    filter: Arc<dyn SharedFilter>,
    states: [SIMDBiQuadState; 2],
    used: bool,
}

/// The shared filter buses of a key: one per set of filter coefficients used
/// by its voices. A voice that joins a bus adds its (zero) filter state to the
/// bus state, so the result matches filtering every voice on its own.
///
/// Voices render into planar (per channel) buffers, which avoids
/// interleaving every voice; each bus is interleaved into the output once.
pub struct FilterBuses {
    channels: usize,
    buses: Vec<FilterBus>,
    dry_used: bool,
    /// Planar buffers of the buses while a key renders, borrowed from the
    /// rendering thread so they stay in its cache: slot 0 holds the voices
    /// without a shared filter, slot `i + 1` the bus `i`.
    scratch: Vec<f32>,
    /// Length of one scratch slot
    slot_len: usize,
}

thread_local! {
    static SCRATCH: std::cell::Cell<Vec<f32>> = const { std::cell::Cell::new(Vec::new()) };
}

/// Unused buses kept around (with a cleared state) to avoid reallocating
const MAX_IDLE_BUSES: usize = 8;

impl FilterBuses {
    pub fn new(channels: usize) -> Self {
        Self {
            channels: channels.clamp(1, 2),
            buses: Vec::new(),
            dry_used: false,
            scratch: Vec::new(),
            slot_len: 0,
        }
    }

    /// Returns the cleared bus buffer for a filter, creating the bus if needed.
    #[inline]
    fn bus_for(
        &mut self,
        filter: &Arc<VoiceFilter>,
        hint: &mut u8,
        frames: usize,
    ) -> (&mut [f32], &mut [f32]) {
        let hinted = self
            .buses
            .get(*hint as usize)
            .is_some_and(|bus| Arc::ptr_eq(&bus.source, filter));
        // Voices of one spawner share the filter handle, so compare the
        // pointer before the coefficients.
        let index = match hinted.then_some(*hint as usize).or_else(|| {
            self.buses
                .iter()
                .position(|bus| Arc::ptr_eq(&bus.source, filter))
        }) {
            Some(index) => index,
            None => {
                let id = filter.filter.id();
                match self.buses.iter().position(|bus| bus.id == id) {
                    Some(index) => index,
                    None => {
                        self.buses.push(FilterBus {
                            source: filter.clone(),
                            id,
                            filter: filter.filter.clone(),
                            states: Default::default(),
                            used: false,
                        });
                        self.buses.len() - 1
                    }
                }
            }
        };
        *hint = index.min(u8::MAX as usize) as u8;
        let first_use = !self.buses[index].used;
        self.buses[index].used = true;
        self.slot(index + 1, first_use, frames)
    }

    /// Returns a scratch slot, cleared on its first use in a block.
    #[inline(always)]
    fn slot(&mut self, slot: usize, clear: bool, frames: usize) -> (&mut [f32], &mut [f32]) {
        let len = self.slot_len;
        let range = slot * len..(slot + 1) * len;
        if self.scratch.len() < range.end {
            self.scratch.resize(range.end, 0.0);
        }
        let buffer = &mut self.scratch[range];
        if clear {
            buffer.fill(0.0);
        }
        split_planar(buffer, frames, self.channels)
    }

    /// Takes the scratch buffer of the current thread for a block of `frames`.
    fn begin(&mut self, frames: usize) {
        self.scratch = SCRATCH.take();
        self.slot_len = frames * self.channels;
    }

    /// Returns the cleared planar buffer for voices without a shared filter.
    #[inline]
    fn dry(&mut self, frames: usize) -> (&mut [f32], &mut [f32]) {
        let first_use = !self.dry_used;
        self.dry_used = true;
        self.slot(0, first_use, frames)
    }

    /// Filters the buses used in this block into `out`. Buses no voice used
    /// lose their state, as their voices ended.
    fn finish(&mut self, out: &mut [f32]) {
        let frames = out.len() / self.channels;
        let len = self.slot_len;
        if self.dry_used {
            self.dry_used = false;
            add_planar(&self.scratch[..len], out, self.channels);
        }
        for (i, bus) in self.buses.iter_mut().enumerate() {
            if bus.used {
                let buffer = &mut self.scratch[(i + 1) * len..(i + 2) * len];
                bus.used = false;
                bus.filter
                    .process_planar(&mut bus.states[..self.channels], buffer, frames);
                add_planar(buffer, out, self.channels);
            } else {
                bus.states = Default::default();
            }
        }
        if self.buses.len() > MAX_IDLE_BUSES {
            self.buses
                .retain(|bus| bus.states.iter().any(|s| *s != SIMDBiQuadState::default()));
        }
        SCRATCH.set(std::mem::take(&mut self.scratch));
    }

    /// Clears every bus state, for when the key has no voices left.
    pub fn reset(&mut self) {
        for bus in self.buses.iter_mut() {
            bus.states = Default::default();
        }
    }
}

/// Splits a planar buffer into its left and right channel. Mono buffers
/// return an empty right channel.
#[inline(always)]
fn split_planar(buffer: &mut [f32], frames: usize, channels: usize) -> (&mut [f32], &mut [f32]) {
    if channels == 2 {
        buffer.split_at_mut(frames)
    } else {
        (buffer, &mut [])
    }
}

/// Adds a planar buffer to an interleaved one.
fn add_planar(planar: &[f32], out: &mut [f32], channels: usize) {
    if channels == 2 {
        let (left, right) = planar.split_at(planar.len() / 2);
        for ((frame, l), r) in out.as_chunks_mut::<2>().0.iter_mut().zip(left).zip(right) {
            frame[0] += l;
            frame[1] += r;
        }
    } else {
        sum_simd(planar, out);
    }
}
