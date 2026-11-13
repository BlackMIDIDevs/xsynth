use std::{io, ops::RangeInclusive, sync::Arc};

use crossbeam_channel::Sender;

use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent, ControlEvent};

use crate::{util::ReadWriteAtomicU64, SynthEvent};

mod nps;
mod sender;

use sender::EventSender;

/// A helper object to send events to the realtime synthesizer.
#[derive(Clone)]
pub struct RealtimeEventSender {
    senders: Vec<EventSender>,
}

impl RealtimeEventSender {
    pub(super) fn new(
        senders: Vec<Sender<ChannelEvent>>,
        max_nps: Arc<ReadWriteAtomicU64>,
        ignore_range: RangeInclusive<u8>,
    ) -> Result<RealtimeEventSender, io::Error> {
        Ok(RealtimeEventSender {
            senders: senders
                .into_iter()
                .map(|sender| EventSender::new(max_nps.clone(), sender, ignore_range.clone()))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// Sends a SynthEvent to the realtime synthesizer.
    ///
    /// See the `SynthEvent` documentation for more information.
    pub fn send_event(&mut self, event: SynthEvent) {
        match event {
            SynthEvent::Channel(channel, event) => self.send_channel_event(channel, event),
            SynthEvent::AllChannels(event) => self.send_all_channels_event(event),
        }
    }

    /// Sends a MIDI event as raw bytes.
    pub fn send_event_u32(&mut self, event: u32) {
        let head = event & 0xFF;
        let channel = head & 0xF;
        let code = head >> 4;

        let val1 = (event >> 8) as u8;
        let val2 = (event >> 16) as u8;

        match code {
            0x8 => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: val1 }),
            )),
            0x9 => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                    key: val1,
                    vel: val2,
                }),
            )),
            0xB => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(val1, val2))),
            )),
            0xC => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(val1)),
            )),
            0xD => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::ChannelPressure(
                    val1,
                ))),
            )),
            0xE => {
                let value = (((val2 as i16) << 7) | val1 as i16) - 8192;
                let value = value as f32 / 8192.0;
                self.send_event(SynthEvent::Channel(
                    channel,
                    ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                        value,
                    ))),
                ));
            }
            _ => {}
        }
    }

    /// Resets all note and control change data of the realtime synthesizer.
    pub fn reset_synth(&mut self) {
        self.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::AllNotesKilled,
        )));

        for sender in &mut self.senders {
            sender.reset_skipped_notes();
        }

        self.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::ResetControl,
        )));
    }

    /// Changes the range of velocities that will be ignored for the
    /// specific sender instance.
    pub fn set_ignore_range(&mut self, ignore_range: RangeInclusive<u8>) {
        for sender in &mut self.senders {
            sender.set_ignore_range(ignore_range.clone());
        }
    }

    fn send_channel_event(&mut self, channel: u32, event: ChannelEvent) {
        let Some(sender) = self.senders.get_mut(channel as usize) else {
            return;
        };

        match event {
            ChannelEvent::Audio(event) => sender.send_audio(event),
            ChannelEvent::Config(event) => sender.send_config(event),
        }
    }

    fn send_all_channels_event(&mut self, event: ChannelEvent) {
        match event {
            ChannelEvent::Audio(event) => {
                for sender in &mut self.senders {
                    sender.send_audio(event);
                }
            }
            ChannelEvent::Config(event) => {
                for sender in &mut self.senders {
                    sender.send_config(event.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use crossbeam_channel::unbounded;
    use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent};

    use super::RealtimeEventSender;
    use crate::{util::ReadWriteAtomicU64, SynthEvent};

    #[test]
    fn cloned_sender_can_be_sent_to_another_thread() {
        let (tx, rx) = unbounded();
        let max_nps = Arc::new(ReadWriteAtomicU64::new(10_000));
        let sender = RealtimeEventSender::new(vec![tx], max_nps, 0..=0).unwrap();

        let mut sender_thread = sender.clone();
        thread::spawn(move || {
            sender_thread.send_event(SynthEvent::Channel(
                0,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key: 64, vel: 127 }),
            ));
        })
        .join()
        .unwrap();

        assert!(matches!(
            rx.recv().unwrap(),
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key: 64, vel: 127 })
        ));
    }

    #[test]
    fn reset_clears_skipped_notes_state() {
        let (tx, rx) = unbounded();
        let max_nps = Arc::new(ReadWriteAtomicU64::new(0));
        let mut sender = RealtimeEventSender::new(vec![tx], max_nps, 0..=0).unwrap();

        sender.send_event(SynthEvent::Channel(
            0,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key: 60, vel: 100 }),
        ));
        sender.send_event(SynthEvent::Channel(
            0,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: 60 }),
        ));

        assert!(rx.is_empty());

        sender.reset_synth();

        assert!(matches!(
            rx.recv().unwrap(),
            ChannelEvent::Audio(ChannelAudioEvent::AllNotesKilled)
        ));
        assert!(matches!(
            rx.recv().unwrap(),
            ChannelEvent::Audio(ChannelAudioEvent::ResetControl)
        ));

        sender.send_event(SynthEvent::Channel(
            0,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: 60 }),
        ));

        assert!(matches!(
            rx.recv().unwrap(),
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: 60 })
        ));
    }

    #[test]
    fn out_of_range_channel_is_ignored() {
        let (tx, rx) = unbounded();
        let max_nps = Arc::new(ReadWriteAtomicU64::new(10_000));
        let mut sender = RealtimeEventSender::new(vec![tx], max_nps, 0..=0).unwrap();

        sender.send_event(SynthEvent::Channel(
            1,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key: 64, vel: 127 }),
        ));

        assert!(rx.is_empty());
    }
}
