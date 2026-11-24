use super::*;
mod actions;
mod controls;
mod cpu_view;
mod menu;
mod overview;
mod rows;
mod views;
mod widgets;
use std::collections::VecDeque;
use std::time::Instant;
#[derive(Clone)]
pub(super) struct Sample {
    pub at: Instant,
    pub cpu: f64,
    pub down: f64,
    pub up: f64,
}
use superlink_terminal::monitor::{LargeFile, MonitorDetails, MonitorRates, ProcessMetric};

pub(in crate::terminal_page) struct State {
    view: usize,
    menu: bool,
    pub(in crate::terminal_page) paused: bool,
    pub(in crate::terminal_page) interval: u64,
    rates: MonitorRates,
    previous: Option<MonitorDetails>,
    history: VecDeque<Sample>,
    paths: Entity<SqlEditor>,
    minimum: u64,
    count: usize,
    files: Vec<LargeFile>,
    confirm: Option<ProcessMetric>,
    busy: bool,
    pub cancel: CancelToken,
    job: u64,
    notice: String,
}
impl State {
    pub fn new(cx: &mut Context<TerminalPage>) -> Self {
        let paths = cx.new(SqlEditor::new);
        paths.update(cx, |e, cx| e.set_text("/home /var /opt", cx));
        Self {
            view: 0,
            menu: false,
            paused: false,
            interval: 3,
            rates: Default::default(),
            previous: None,
            history: VecDeque::new(),
            paths,
            minimum: 100 << 20,
            count: 20,
            files: Vec::new(),
            confirm: None,
            busy: false,
            cancel: CancelToken::new(),
            job: 0,
            notice: String::new(),
        }
    }
    pub fn reset(&mut self) {
        self.cancel.cancel();
        self.cancel = CancelToken::new();
        self.job = self.job.wrapping_add(1);
        self.previous = None;
        self.rates = Default::default();
        self.history.clear();
        self.files.clear();
        self.confirm = None;
        self.busy = false;
        self.notice.clear();
    }
    pub fn update(&mut self, metrics: &Metrics, local_rates: Option<MonitorRates>) {
        self.rates = local_rates.unwrap_or_else(|| {
            self.previous
                .as_ref()
                .map(|previous| superlink_terminal::monitor::rates(previous, &metrics.details))
                .unwrap_or_default()
        });
        self.previous = Some(metrics.details.clone());
        if self.rates.ready {
            let (down, up) = host_totals(&self.rates);
            let at = metrics.details.sampled.unwrap_or_else(Instant::now);
            append_sample(
                &mut self.history,
                Sample {
                    at,
                    cpu: cpu_total(&self.rates),
                    down,
                    up,
                },
            );
        }
    }
}
fn append_sample(history: &mut VecDeque<Sample>, sample: Sample) {
    let at = sample.at;
    history.push_back(sample);
    while history.len() > 60
        || history
            .front()
            .is_some_and(|p| at.saturating_duration_since(p.at).as_secs_f64() > 60.)
    {
        history.pop_front();
    }
}
fn cpu_total(rates: &MonitorRates) -> f64 {
    rates
        .cpus
        .iter()
        .find(|c| c.name == "cpu")
        .map(|c| c.used)
        .unwrap_or_else(|| {
            rates.cpus.iter().map(|c| c.used).sum::<f64>() / rates.cpus.len().max(1) as f64
        })
}
fn host_totals(rates: &MonitorRates) -> (f64, f64) {
    rates
        .networks
        .iter()
        .filter(|n| host_interface(&n.name))
        .fold((0., 0.), |(down, up), n| (down + n.received, up + n.sent))
}
fn host_interface(name: &str) -> bool {
    !["lo", "docker", "veth", "br-", "virbr", "vmnet"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_cores_average_and_remote_aggregate_is_not_double_counted() {
        let mut r = MonitorRates {
            cpus: vec![
                superlink_terminal::monitor::rates::CpuRate {
                    name: "cpu0".into(),
                    used: 20.,
                    ..Default::default()
                },
                superlink_terminal::monitor::rates::CpuRate {
                    name: "cpu1".into(),
                    used: 40.,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(cpu_total(&r), 30.);
        r.cpus.push(superlink_terminal::monitor::rates::CpuRate {
            name: "cpu".into(),
            used: 25.,
            ..Default::default()
        });
        assert_eq!(cpu_total(&r), 25.);
        assert!(!host_interface("veth123"));
        assert!(host_interface("eth0"));
    }
    #[test]
    fn history_is_bounded_by_elapsed_time_when_remote_probes_are_slow() {
        let mut history = VecDeque::new();
        let start = Instant::now();
        for n in 0..100 {
            append_sample(
                &mut history,
                Sample {
                    at: start + Duration::from_secs(n * 10),
                    cpu: 1.,
                    down: 2.,
                    up: 3.,
                },
            );
        }
        assert_eq!(history.len(), 7);
        assert_eq!(
            history
                .back()
                .unwrap()
                .at
                .duration_since(history.front().unwrap().at)
                .as_secs(),
            60
        );
    }
}
