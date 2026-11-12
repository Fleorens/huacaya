//! The snapshots cross the network between `llamatop agent` and its clients as JSON: they must come back
//! equal to what was sent.

use llamatop::app::demo::{self, DemoState};
use llamatop::collect::llama_poll::LlamaSnapshot;
use llamatop::collect::logs::LogLine;
use llamatop::collect::sampler::SystemSnapshot;

#[test]
fn snapshots_survive_json() {
    for state in
        [DemoState::Idle, DemoState::Prompt, DemoState::Generating, DemoState::Loading, DemoState::Down]
    {
        let system = demo::system(17.3, state);
        let back: SystemSnapshot = serde_json::from_str(&serde_json::to_string(&system).unwrap()).unwrap();
        assert_eq!(back, system);
        let llama = demo::llama(17.3, state);
        let back: LlamaSnapshot = serde_json::from_str(&serde_json::to_string(&llama).unwrap()).unwrap();
        assert_eq!(back, llama);
    }
    let lines = demo::log_lines(0.0, 40.0);
    let back: Vec<LogLine> = serde_json::from_str(&serde_json::to_string(&lines).unwrap()).unwrap();
    assert_eq!(back, lines);
}
