use std::path::PathBuf;

use self::{parse::parse_tokens_resolved, region::parse_sf_root};

mod grammar;
mod parse;
mod region;
pub use parse::{SfzParseError, SfzValidationError};
pub use region::{AmpegEnvelopeParams, RegionParams};

/// Parses an SFZ file and returns its regions in a vector.
pub fn parse_soundfont(sfz_path: impl Into<PathBuf>) -> Result<Vec<RegionParams>, SfzParseError> {
    let sfz_path = sfz_path.into();
    let sfz_path: PathBuf = sfz_path
        .canonicalize()
        .map_err(|_| SfzParseError::FailedToReadFile(sfz_path))?;

    let tokens = parse_tokens_resolved(&sfz_path)?;

    // Unwrap here is safe because the path is confirmed to be a file due to `parse_all_tokens`
    // and therefore it will always have a parent folder. The path is also canonicalized.
    let parent_path = sfz_path.parent().unwrap().into();

    let regions = parse_sf_root(tokens.into_iter(), parent_path);

    Ok(regions)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::parse_soundfont;

    fn create_temp_dir(label: &str) -> PathBuf {
        let unique = format!(
            "xsynth-sfz-test-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn parse_soundfont_resolves_includes_and_defines() {
        let dir = create_temp_dir("include-define");
        let sfz_path = dir.join("instrument.sfz");
        let inc_path = dir.join("include.sfz");
        let sample_path = dir.join("samples").join("snare.wav");

        write(&sample_path, "");
        write(
            &inc_path,
            r#"
<group>
lovel=1
sample=$SAMPLE_NAME
"#,
        );
        write(
            &sfz_path,
            r#"
#define $SAMPLE_NAME snare.wav
<control>
default_path=samples/
#include "include.sfz"
<region>
hivel=80
"#,
        );

        let regions = parse_soundfont(&sfz_path).unwrap();

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].velrange, 1..=80);
        assert_eq!(regions[0].sample_path, sample_path.canonicalize().unwrap());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parse_soundfont_inherits_parent_group_values() {
        let dir = create_temp_dir("inheritance");
        let sfz_path = dir.join("instrument.sfz");
        let sample_path = dir.join("samples").join("tone.wav");

        write(&sample_path, "");
        write(
            &sfz_path,
            r#"
<global>
lokey=c3
<group>
hikey=d3
volume=-6
<region>
sample=samples/tone.wav
"#,
        );

        let regions = parse_soundfont(&sfz_path).unwrap();

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].keyrange, 48..=50);
        assert_eq!(regions[0].volume, -6);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parse_soundfont_reads_controller_opcodes() {
        use crate::modulation::{
            CcCrossfade, CcRange, CrossfadeCurve, EnvelopeTarget, ModController, ModDestination,
        };

        let dir = create_temp_dir("controllers");
        let sfz_path = dir.join("instrument.sfz");
        let sample_path = dir.join("tone.wav");

        write(&sample_path, "");
        write(
            &sfz_path,
            r#"
<control>
set_cc1=20
set_hdcc11=0.5
<group>
amplitude=0
amplitude_oncc1=100
volume_oncc7=-12
gain_cc11=6
pan_oncc10=50
pitch_oncc128=200
tune_cc2=100
cutoff_oncc74=1200
resonance_oncc71=6
ampeg_attack_oncc73=2
ampeg_releasecc72=1.5
cutoff_chanaft=600
locc64=64
hicc64=127
xfin_locc1=0
xfin_hicc1=64
xf_cccurve=gain
effect1=40
effect2=10
<region>
sample=tone.wav
<region>
sample=tone.wav
on_locc64=100
on_hicc64=127
"#,
        );

        let regions = parse_soundfont(&sfz_path).unwrap();
        assert_eq!(regions.len(), 2);
        let r = &regions[0];

        assert_eq!(r.amplitude, 0.0);
        assert_eq!(r.set_cc, vec![(1, 20), (11, 64)]);
        assert_eq!(r.reverb_send, Some(40.0));
        assert_eq!(r.chorus_send, Some(10.0));
        assert_eq!(
            r.cc_conditions,
            vec![CcRange {
                cc: 64,
                lo: 64,
                hi: 127
            }]
        );
        assert!(r.cc_triggers.is_empty());
        assert_eq!(
            r.modulation.crossfades,
            vec![CcCrossfade {
                cc: 1,
                lo: 0,
                hi: 64,
                fade_in: true
            }]
        );
        assert_eq!(r.modulation.crossfade_curve, CrossfadeCurve::Gain);

        let find = |dest: ModDestination, controller: ModController| {
            r.modulation
                .modulators
                .iter()
                .find(|m| m.destination == dest && m.source.controller == controller)
                .map(|m| (m.amount, m.source.bipolar))
        };
        assert_eq!(
            find(ModDestination::AmplitudePercent, ModController::Cc(1)),
            Some((100.0, false))
        );
        assert_eq!(
            find(ModDestination::VolumeDb, ModController::Cc(7)),
            Some((-12.0, false))
        );
        assert_eq!(
            find(ModDestination::VolumeDb, ModController::Cc(11)),
            Some((6.0, false))
        );
        assert_eq!(
            find(ModDestination::Pan, ModController::Cc(10)),
            Some((0.5, false))
        );
        assert_eq!(
            find(ModDestination::PitchCents, ModController::PitchWheel),
            Some((200.0, true))
        );
        assert_eq!(
            find(ModDestination::PitchCents, ModController::Cc(2)),
            Some((100.0, false))
        );
        assert_eq!(
            find(ModDestination::CutoffCents, ModController::Cc(74)),
            Some((1200.0, false))
        );
        assert_eq!(
            find(ModDestination::CutoffCents, ModController::ChannelPressure),
            Some((600.0, false))
        );
        assert_eq!(
            find(ModDestination::ResonanceDb, ModController::Cc(71)),
            Some((6.0, false))
        );
        assert_eq!(
            find(
                ModDestination::EnvelopeLinear(EnvelopeTarget::Attack),
                ModController::Cc(73)
            ),
            Some((2.0, false))
        );
        assert_eq!(
            find(
                ModDestination::EnvelopeLinear(EnvelopeTarget::Release),
                ModController::Cc(72)
            ),
            Some((1.5, false))
        );

        assert_eq!(
            regions[1].cc_triggers,
            vec![CcRange {
                cc: 64,
                lo: 100,
                hi: 127
            }]
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
