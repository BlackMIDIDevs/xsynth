use super::Sf2Zone;
use crate::LoopMode;
use soundfont::{data::hydra::generator::GeneratorType, Instrument};

#[derive(Clone, Debug)]
pub struct Sf2Instrument {
    pub regions: Vec<Sf2Zone>,
}

impl Sf2Instrument {
    pub fn parse_instruments(instruments: Vec<Instrument>) -> Vec<Self> {
        let mut out: Vec<Self> = Vec::new();

        for instrument in instruments {
            let mut regions: Vec<Sf2Zone> = Vec::new();
            let mut global_region = Sf2Zone::default();

            for (i, zone) in instrument.zones.iter().enumerate() {
                let mut region = global_region.clone();

                for gen in &zone.gen_list {
                    match gen.ty {
                        GeneratorType::StartAddrsOffset => {
                            region.offset = gen.amount.as_i16().copied()
                        }
                        GeneratorType::StartloopAddrsOffset => {
                            region.loop_start_offset = gen.amount.as_i16().copied()
                        }
                        GeneratorType::EndloopAddrsOffset => {
                            region.loop_end_offset = gen.amount.as_i16().copied()
                        }
                        GeneratorType::InitialFilterFc => {
                            region.cutoff = gen.amount.as_i16().copied()
                        }
                        GeneratorType::InitialFilterQ => {
                            region.resonance = gen.amount.as_i16().copied()
                        }
                        GeneratorType::Pan => region.pan = gen.amount.as_i16().copied(),
                        GeneratorType::DelayVolEnv => {
                            region.env_delay =
                                gen.amount.as_i16().map(|v| 2f32.powf(*v as f32 / 1200.0))
                        }
                        GeneratorType::AttackVolEnv => {
                            region.env_attack =
                                gen.amount.as_i16().map(|v| 2f32.powf(*v as f32 / 1200.0))
                        }
                        GeneratorType::HoldVolEnv => {
                            region.env_hold =
                                gen.amount.as_i16().map(|v| 2f32.powf(*v as f32 / 1200.0))
                        }
                        GeneratorType::DecayVolEnv => {
                            region.env_decay =
                                gen.amount.as_i16().map(|v| 2f32.powf(*v as f32 / 1200.0))
                        }
                        GeneratorType::SustainVolEnv => {
                            region.env_sustain = gen
                                .amount
                                .as_i16()
                                .map(|v| 10f32.powf(-1.0 * *v as f32 / 200.0) * 100.0)
                        }
                        GeneratorType::ReleaseVolEnv => {
                            region.env_release =
                                gen.amount.as_i16().map(|v| 2f32.powf(*v as f32 / 1200.0))
                        }
                        GeneratorType::KeyRange => {
                            let range = gen.amount.as_range().copied();
                            region.keyrange = range.map(|v| v.low..=v.high)
                        }
                        GeneratorType::VelRange => {
                            let range = gen.amount.as_range().copied();
                            region.velrange = range.map(|v| v.low..=v.high)
                        }
                        GeneratorType::InitialAttenuation => {
                            region.attenuation = gen.amount.as_i16().copied()
                        }
                        GeneratorType::CoarseTune => {
                            region.coarse_tune = gen.amount.as_i16().copied()
                        }
                        GeneratorType::FineTune => region.fine_tune = gen.amount.as_i16().copied(),
                        GeneratorType::SampleID => region.index = gen.amount.as_u16().copied(),
                        GeneratorType::SampleModes => {
                            region.loop_mode = gen.amount.as_i16().map(|v| match v {
                                1 => LoopMode::LoopContinuous,
                                3 => LoopMode::LoopSustain,
                                _ => LoopMode::NoLoop,
                            })
                        }
                        GeneratorType::OverridingRootKey => {
                            region.root_override = gen.amount.as_i16().copied()
                        }
                        _ => {}
                    }
                }

                if i == 0 && region.index.is_none() {
                    global_region = region;
                } else {
                    regions.push(region);
                }
            }

            out.push(Sf2Instrument { regions });
        }
        out
    }
}
