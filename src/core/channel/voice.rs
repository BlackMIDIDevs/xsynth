pub trait Voice: Send + Sync {
    fn is_ended(&self) -> bool;
    fn is_releasing(&self) -> bool;
    fn signal_release(&mut self);

    fn render_to(&mut self, buffer: &mut [f32]);
}