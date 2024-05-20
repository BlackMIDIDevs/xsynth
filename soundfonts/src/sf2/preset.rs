use super::{instrument::Sf2Instrument, sample::Sf2Sample, Sf2Preset, Sf2Region, Sf2Zone};
use crate::{sfz::AmpegEnvelopeParams, LoopMode};
use soundfont::{data::hydra::generator::GeneratorType, Preset};
use std::ops::RangeInclusive;

#[derive(Clone, Debug)]
pub struct Sf2ParsedPreset {
    pub bank: u16,
    pub preset: u16,
    pub zones: Vec<Sf2Zone>,
}

impl Sf2ParsedPreset {
    pub fn parse_presets(presets: Vec<Preset>) -> Vec<Sf2ParsedPreset> {
        let mut presets_parsed: Vec<Sf2ParsedPreset> = Vec::new();

        for preset in presets {
            let mut zones: Vec<Sf2Zone> = Vec::new();
            let mut global_region = Sf2Zone::default();

            for (i, zone) in preset.zones.iter().enumerate() {
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
                        GeneratorType::Instrument => region.index = gen.amount.as_u16().copied(),
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
                    zones.push(region);
                }
            }
            presets_parsed.push(Sf2ParsedPreset {
                preset: preset.header.preset,
                bank: preset.header.bank,
                zones,
            });
        }

        presets_parsed
    }

    pub fn merge_presets(
        sample_data: Vec<Sf2Sample>,
        instruments: Vec<Sf2Instrument>,
        presets: Vec<Sf2ParsedPreset>,
    ) -> Vec<Sf2Preset> {
        let mut out: Vec<Sf2Preset> = Vec::new();

        for preset in presets {
            let mut new_preset = Sf2Preset {
                preset: preset.preset,
                bank: preset.bank,
                regions: Vec::new(),
            };

            for zone in preset.zones {
                if let Some(instrument_idx) = zone.index {
                    let instrument = &instruments[instrument_idx as usize];

                    for subzone in &instrument.regions {
                        if let Some(sample_idx) = subzone.index {
                            let sample = &sample_data[sample_idx as usize];

                            let new_region = Sf2Region {
                                sample: sample.data.clone(),
                                sample_rate: sample.sample_rate,
                                velrange: combine_ranges(
                                    zone.velrange.clone().unwrap_or(0..=127),
                                    subzone.velrange.clone().unwrap_or(0..=127),
                                ),
                                keyrange: combine_ranges(
                                    zone.keyrange.clone().unwrap_or(0..=127),
                                    subzone.keyrange.clone().unwrap_or(0..=127),
                                ),
                                root_key: subzone.root_override.unwrap_or(sample.origpitch as i16)
                                    as u8,
                                volume: {
                                    let v = zone
                                        .attenuation
                                        .unwrap_or(subzone.attenuation.unwrap_or(0));
                                    10f32.powf(-0.4 * v as f32 / 200.0)
                                },
                                pan: zone.pan.unwrap_or(subzone.pan.unwrap_or(0)),
                                loop_mode: zone
                                    .loop_mode
                                    .unwrap_or(subzone.loop_mode.unwrap_or(LoopMode::NoLoop)),
                                loop_start: (sample.loop_start as i32
                                    + subzone.loop_start_offset.unwrap_or(0) as i32)
                                    as u32,
                                loop_end: (sample.loop_end as i32
                                    + subzone.loop_end_offset.unwrap_or(0) as i32)
                                    as u32,
                                offset: subzone.offset.unwrap_or(0) as u32,
                                cutoff: subzone.cutoff.map(|v| {
                                    2f32.powf(v as f32 / 1200.0)
                                        * 8.176
                                        * 2f32.powf(zone.cutoff.unwrap_or(0) as f32 / 1200.0)
                                }),
                                resonance: zone.resonance.unwrap_or(subzone.resonance.unwrap_or(0))
                                    as f32
                                    / 10.0,
                                fine_tune: zone.fine_tune.unwrap_or(subzone.fine_tune.unwrap_or(0))
                                    + sample.pitchadj as i16,
                                coarse_tune: zone
                                    .coarse_tune
                                    .unwrap_or(subzone.coarse_tune.unwrap_or(0)),
                                ampeg_envelope: AmpegEnvelopeParams {
                                    ampeg_start: 0.0,
                                    ampeg_delay: subzone.env_delay.unwrap_or(0.0)
                                        * zone.env_delay.unwrap_or(1.0),
                                    ampeg_attack: subzone.env_attack.unwrap_or(0.0)
                                        * zone.env_attack.unwrap_or(1.0),
                                    ampeg_hold: subzone.env_hold.unwrap_or(0.0)
                                        * zone.env_hold.unwrap_or(1.0),
                                    ampeg_decay: subzone.env_decay.unwrap_or(0.0)
                                        * zone.env_decay.unwrap_or(1.0),
                                    ampeg_sustain: zone
                                        .env_sustain
                                        .unwrap_or(subzone.env_sustain.unwrap_or(100.0)),
                                    ampeg_release: subzone.env_release.unwrap_or(0.0)
                                        * zone.env_release.unwrap_or(1.0),
                                },
                            };

                            new_preset.regions.push(new_region);
                        }
                    }
                }
            }

            out.push(new_preset);
        }

        out
    }
}

fn combine_ranges<T: Ord + Copy>(
    r1: RangeInclusive<T>,
    r2: RangeInclusive<T>,
) -> RangeInclusive<T> {
    let start1 = r1.start();
    let start2 = r2.start();
    let end1 = r1.end();
    let end2 = r2.end();

    (*start1.max(start2))..=(*end1.min(end2))
}
