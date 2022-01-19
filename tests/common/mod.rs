use std::sync::Once;

static LOG: Once = Once::new();

pub fn setup() {
    LOG.call_once(|| {
        env_logger::init();
    });
}
