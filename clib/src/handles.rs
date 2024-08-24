use std::{ffi::c_void, sync::Arc};
use xsynth_core::{
    channel_group::ChannelGroup,
    soundfont::{SampleSoundfont, SoundfontBase},
};
use xsynth_realtime::RealtimeSynth;

/// Handle of an internal ChannelGroup instance in XSynth.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XSynth_ChannelGroup {
    pub group: *mut c_void,
}

impl XSynth_ChannelGroup {
    pub(crate) fn from(group: ChannelGroup) -> Self {
        let group = Box::into_raw(Box::new(group));
        Self {
            group: group as *mut c_void,
        }
    }

    pub(crate) fn drop(self) {
        let group = self.group as *mut ChannelGroup;
        unsafe { drop(Box::from_raw(group)) }
    }

    pub(crate) fn as_ref(&self) -> &ChannelGroup {
        let group = self.group as *mut ChannelGroup;
        unsafe { &*group }
    }

    #[allow(clippy::mut_from_ref)]
    pub(crate) fn as_mut(&self) -> &mut ChannelGroup {
        let group = self.group as *mut ChannelGroup;
        unsafe { &mut *group }
    }
}

/// Handle of an internal Soundfont object in XSynth.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XSynth_Soundfont {
    pub soundfont: *mut c_void,
}

impl XSynth_Soundfont {
    pub(crate) fn from(sf: Arc<SampleSoundfont>) -> Self {
        let sf = Box::into_raw(Box::new(sf));
        Self {
            soundfont: sf as *mut c_void,
        }
    }

    pub(crate) fn drop(self) {
        let soundfont = self.soundfont as *mut Arc<SampleSoundfont>;
        unsafe { drop(Box::from_raw(soundfont)) }
    }

    pub(crate) fn clone(&self) -> Arc<dyn SoundfontBase> {
        unsafe {
            let sf = self.soundfont as *mut Arc<SampleSoundfont>;
            let sf = &*sf;
            sf.clone()
        }
    }
}

/// Handle of an internal RealtimeSynth instance in XSynth.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XSynth_RealtimeSynth {
    pub synth: *mut c_void,
}

impl XSynth_RealtimeSynth {
    pub(crate) fn from(synth: RealtimeSynth) -> Self {
        let synth = Box::into_raw(Box::new(synth));
        Self {
            synth: synth as *mut c_void,
        }
    }

    pub(crate) fn drop(self) {
        let synth = self.synth as *mut RealtimeSynth;
        unsafe { drop(Box::from_raw(synth)) }
    }

    pub(crate) fn as_ref(&self) -> &RealtimeSynth {
        let synth = self.synth as *mut RealtimeSynth;
        unsafe { &*synth }
    }

    #[allow(clippy::mut_from_ref)]
    pub(crate) fn as_mut(&self) -> &mut RealtimeSynth {
        let synth = self.synth as *mut RealtimeSynth;
        unsafe { &mut *synth }
    }
}
