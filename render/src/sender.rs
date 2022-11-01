use crossbeam_channel::Sender;
use crate::SynthEvent;
use core::channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent};

struct EventSender {
    sender: Sender<ChannelEvent>,
}

impl EventSender {
    pub fn new(sender: Sender<ChannelEvent>) -> Self {
        EventSender {
            sender,
        }
    }

    pub fn send_audio(&mut self, event: ChannelAudioEvent) {
        self.sender.send(ChannelEvent::Audio(event)).ok();
    }

    pub fn send_config(&mut self, event: ChannelConfigEvent) {
        self.sender.send(ChannelEvent::Config(event)).ok();
    }
}

impl Clone for EventSender {
    fn clone(&self) -> Self {
        EventSender {
            sender: self.sender.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RenderEventSender {
    senders: Vec<EventSender>,
}

impl RenderEventSender {
    pub fn new(
        senders: Vec<Sender<ChannelEvent>>,
    ) -> Self {
        Self {
            senders: senders
                .into_iter()
                .map(|s| EventSender::new(s))
                .collect(),
        }
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        match event {
            SynthEvent::Channel(channel, event) => {
                if channel != 9 {
                    self.senders[channel as usize].send_audio(event);
                }
            }
            SynthEvent::AllChannels(event) => {
                for sender in self.senders.iter_mut() {
                    sender.send_audio(event.clone());
                }
            }
            SynthEvent::ChannelConfig(event) => {
                for sender in self.senders.iter_mut() {
                    sender.send_config(event.clone());
                }
            }
        }
    }

    pub fn send_config(&mut self, event: ChannelConfigEvent) {
        self.send_event(SynthEvent::ChannelConfig(event))
    }

    pub fn send_event_u32(&mut self, event: u32) {
        let head = event & 0xFF;
        let channel = head & 0xF;
        let code = head >> 4;

        macro_rules! val1 {
            () => {
                (event >> 8) as u8
            };
        }

        macro_rules! val2 {
            () => {
                (event >> 16) as u8
            };
        }

        match code {
            0x8 => {
                self.send_event(SynthEvent::Channel(
                    channel,
                    ChannelAudioEvent::NoteOff { key: val1!() },
                ));
            }
            0x9 => {
                self.send_event(SynthEvent::Channel(
                    channel,
                    ChannelAudioEvent::NoteOn {
                        key: val1!(),
                                                    vel: val2!(),
                    },
                ));
            }
            0xB => {
                self.send_event(SynthEvent::Channel(
                    channel,
                    ChannelAudioEvent::Control(ControlEvent::Raw(val1!(), val2!())),
                ));
            }
            0xE => {
                let value = (((val2!() as i16) << 7) | val1!() as i16) - 8192;
                let value = value as f32 / 8192.0;
                self.send_event(SynthEvent::Channel(
                    channel,
                    ChannelAudioEvent::Control(ControlEvent::PitchBendValue(value)),
                ));
            }

            _ => {}
        }
    }

    pub fn reset_synth(&mut self) {
        self.send_event(SynthEvent::AllChannels(ChannelAudioEvent::AllNotesKilled));
    }
}
