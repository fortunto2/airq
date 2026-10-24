//! Shift plans for outdoor crews: an hourly smoke and heat forecast in, the actions a jurisdiction's
//! rules require out, at the hour they start.
//!
//! The engine knows no law. Thresholds, wording and citations live in a rulebook (`rulebooks/*.json`),
//! one per jurisdiction; California's ships embedded.
//!
//! The one piece of science here is the NowCast. Cal/OSHA §5141.1 is written against the *current*
//! AQI, which AirNow computes as a NowCast over the last 12 hours. A model's `us_aqi` is usually the
//! 24-hour average (Open-Meteo's is), and that lags a smoke front by most of a shift: measured on
//! Open-Meteo, a spike to 55 µg/m³ in the latest hour reads AQI 149 as a NowCast and ~55 as a daily
//! mean. So the forecast hands in raw hourly PM2.5 and the AQI is made here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One forecast hour as an upstream gives it. `time` is local ISO, `2026-10-24T13:00`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawHour {
    pub time: String,
    pub pm25: Option<f64>,
    pub temp_c: Option<f64>,
}

/// One hour with every metric a rulebook may name.
#[derive(Debug, Clone, Serialize)]
pub struct Hour {
    pub time: String,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    pub id: String,
    pub group: String,
    pub metric: String,
    pub gte: f64,
    /// `{value}` is the metric, `{aqi}` the hour's NowCast AQI.
    pub action: String,
    pub cite: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rulebook {
    pub jurisdiction: String,
    pub name: String,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Action {
    pub time: String,
    pub rule: String,
    pub cite: String,
    pub text: String,
}

/// What a building's air handler should do with outside air (Track 3 of the hackathon: BMS).
#[derive(Debug, Clone, Serialize)]
pub struct BmsSignal {
    pub threshold_aqi: u32,
    /// First hour the NowCast AQI reaches the threshold: close outside-air dampers, recirculate.
    pub recirculate_at: Option<String>,
    /// First hour after that it drops back below: open them again.
    pub resume_at: Option<String>,
    pub peak_aqi: Option<u32>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShiftPlan {
    pub jurisdiction: String,
    pub hours: Vec<Hour>,
    pub actions: Vec<Action>,
    pub peak_aqi: Option<u32>,
    pub peak_temp_f: Option<f64>,
    pub bms: BmsSignal,
}

pub const METRICS: [&str; 5] = ["us_aqi_pm25", "pm25_nowcast_ugm3", "pm25_ugm3", "temp_f", "temp_c"];

/// California, outdoor workplaces: §5141.1 smoke, §3395 heat.
pub fn us_ca() -> Rulebook {
    serde_json::from_str(include_str!("../rulebooks/us-ca.json")).expect("embedded rulebook parses")
}

/// EPA NowCast for PM2.5, `recent_first[0]` being the latest hour, up to 12 hours used.
///
/// Weight factor w = max(min/max, 0.5); NowCast = Σ wⁱ·cᵢ / Σ wⁱ over valid hours. Needs two of
/// the three latest hours, as AirNow does; otherwise there is no current value.
pub fn nowcast_pm25(recent_first: &[Option<f64>]) -> Option<f64> {
    let window = &recent_first[..recent_first.len().min(12)];
    if window.iter().take(3).filter(|c| c.is_some()).count() < 2 {
        return None;
    }
    let valid: Vec<f64> = window.iter().flatten().map(|c| c.max(0.0)).collect();
    let max = valid.iter().cloned().fold(0.0, f64::max);
    let min = valid.iter().cloned().fold(f64::INFINITY, f64::min);
    let w = if max > 0.0 { (min / max).max(0.5) } else { 1.0 };
    let (mut num, mut den) = (0.0, 0.0);
    for (i, c) in window.iter().enumerate() {
        if let Some(c) = c {
            let k = w.powi(i as i32);
            num += k * c.max(0.0);
            den += k;
        }
    }
    // EPA truncates the NowCast to 0.1 µg/m³ before it is turned into an AQI.
    Some((num / den * 10.0 + 1e-9).floor() / 10.0)
}

/// NowCast at every hour of a chronological series.
pub fn nowcast_series(pm25: &[Option<f64>]) -> Vec<Option<f64>> {
    (0..pm25.len())
        .map(|i| {
            let back: Vec<Option<f64>> = pm25[..=i].iter().rev().cloned().collect();
            nowcast_pm25(&back)
        })
        .collect()
}

pub fn metrics(raw: &[RawHour]) -> Vec<Hour> {
    let pm: Vec<Option<f64>> = raw.iter().map(|h| h.pm25).collect();
    let now = nowcast_series(&pm);
    raw.iter()
        .zip(now)
        .map(|(h, nc)| {
            let mut m = BTreeMap::new();
            if let Some(v) = h.pm25 {
                m.insert("pm25_ugm3".into(), v);
            }
            if let Some(v) = nc {
                m.insert("pm25_nowcast_ugm3".into(), v);
                m.insert("us_aqi_pm25".into(), crate::pm25_aqi(v) as f64);
            }
            if let Some(c) = h.temp_c {
                m.insert("temp_c".into(), c);
                m.insert("temp_f".into(), c * 9.0 / 5.0 + 32.0);
            }
            Hour { time: h.time.clone(), metrics: m }
        })
        .collect()
}

fn crossed(h: &Hour, r: &Rule) -> bool {
    h.metrics.get(&r.metric).is_some_and(|v| *v >= r.gte)
}

fn render(r: &Rule, h: &Hour) -> String {
    let v = h.metrics[&r.metric];
    let value = if r.metric.ends_with("ugm3") { format!("{v:.1}") } else { format!("{v:.0}") };
    let aqi = h.metrics.get("us_aqi_pm25").map_or("?".into(), |a| format!("{a:.0}"));
    r.action.replace("{value}", &value).replace("{aqi}", &aqi)
}

/// For each group, the most severe rule crossed in the shift, at the first hour it is crossed; then
/// the clear hours (no rule crossed) as the window for heavy work.
pub fn plan(hours: &[Hour], book: &Rulebook) -> Vec<Action> {
    let mut groups: Vec<&str> = Vec::new();
    for r in &book.rules {
        if !groups.contains(&r.group.as_str()) {
            groups.push(&r.group);
        }
    }
    let mut out = Vec::new();
    for g in groups {
        let mut rules: Vec<&Rule> = book.rules.iter().filter(|r| r.group == g).collect();
        rules.sort_by(|a, b| b.gte.total_cmp(&a.gte));
        for r in rules {
            if let Some(h) = hours.iter().find(|h| crossed(h, r)) {
                out.push(Action { time: h.time.clone(), rule: r.id.clone(), cite: r.cite.clone(), text: render(r, h) });
                break;
            }
        }
    }
    let clear: Vec<&str> = hours
        .iter()
        .filter(|h| !book.rules.iter().any(|r| crossed(h, r)))
        .map(|h| clock(&h.time))
        .collect();
    if let (false, Some(first), Some(last)) = (out.is_empty(), clear.first(), clear.last()) {
        out.push(Action {
            time: hours[0].time.clone(),
            rule: "plan".into(),
            cite: String::new(),
            text: format!("Heavy work in clear hours: {first}–{last}"),
        });
    }
    out.sort_by(|a, b| a.time.cmp(&b.time));
    out
}

fn clock(t: &str) -> &str {
    t.get(11..16).unwrap_or(t)
}

fn hour_of(t: &str) -> Option<u32> {
    t.get(11..13)?.parse().ok()
}

fn peak_aqi(hours: &[Hour]) -> Option<u32> {
    hours.iter().filter_map(|h| h.metrics.get("us_aqi_pm25")).map(|v| *v as u32).max()
}

pub fn bms(day: &[Hour], threshold_aqi: u32) -> BmsSignal {
    let aqi = |h: &Hour| h.metrics.get("us_aqi_pm25").map(|v| *v as u32);
    let start = day.iter().position(|h| aqi(h).is_some_and(|a| a >= threshold_aqi));
    let resume = start.and_then(|s| day[s..].iter().find(|h| aqi(h).is_some_and(|a| a < threshold_aqi)));
    let peak = peak_aqi(day);
    let reason = match (start, peak) {
        (Some(s), Some(p)) => format!(
            "outdoor PM2.5 NowCast AQI reaches {threshold_aqi} at {}, peak {p}: close outside air, recirculate through MERV 13+",
            clock(&day[s].time)
        ),
        (None, Some(p)) => format!("outdoor AQI stays below {threshold_aqi} (peak {p}): normal ventilation"),
        _ => "no PM2.5 forecast".into(),
    };
    BmsSignal {
        threshold_aqi,
        recirculate_at: start.map(|s| day[s].time.clone()),
        resume_at: resume.map(|h| h.time.clone()),
        peak_aqi: peak,
        reason,
    }
}

/// The whole answer for one day and shift. `raw` should start a day early so the first shift hour
/// has its 12 hours of NowCast history.
pub fn shift_plan(raw: &[RawHour], date: &str, start: u32, end: u32, book: &Rulebook) -> ShiftPlan {
    let all = metrics(raw);
    let day: Vec<Hour> = all.iter().filter(|h| h.time.starts_with(date)).cloned().collect();
    let shift: Vec<Hour> = day
        .iter()
        .filter(|h| hour_of(&h.time).is_some_and(|x| start <= x && x < end))
        .cloned()
        .collect();
    ShiftPlan {
        jurisdiction: book.jurisdiction.clone(),
        actions: plan(&shift, book),
        peak_aqi: peak_aqi(&shift),
        peak_temp_f: shift.iter().filter_map(|h| h.metrics.get("temp_f").copied()).reduce(f64::max),
        bms: bms(&day, 101),
        hours: shift,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(hour: u32, pm25: f64, temp_c: f64) -> RawHour {
        RawHour { time: format!("2026-10-24T{hour:02}:00"), pm25: Some(pm25), temp_c: Some(temp_c) }
    }

    #[test]
    fn nowcast_of_a_steady_level_is_that_level() {
        assert_eq!(nowcast_pm25(&[Some(40.0); 12]), Some(40.0));
        assert_eq!(crate::pm25_aqi(40.0), 112);
    }

    #[test]
    fn nowcast_follows_a_spike_the_daily_mean_hides() {
        // Latest hour 100, eleven hours of 10 before it: w = 0.5,
        // (100 + 10·Σ½ⁱ, i=1..11) / Σ½ⁱ, i=0..11 = 109.9951 / 1.99951 = 55.01 → 55.0.
        let mut h = vec![Some(100.0)];
        h.extend([Some(10.0); 11]);
        assert_eq!(nowcast_pm25(&h), Some(55.0));
        assert_eq!(crate::pm25_aqi(55.0), 149);
    }

    #[test]
    fn nowcast_needs_two_of_the_last_three_hours() {
        assert_eq!(nowcast_pm25(&[Some(30.0), None, None, Some(30.0)]), None);
        assert_eq!(nowcast_pm25(&[Some(30.0), None, Some(30.0)]), Some(30.0));
        assert_eq!(nowcast_pm25(&[Some(30.0)]), None);
    }

    #[test]
    fn california_rulebook_loads() {
        let b = us_ca();
        assert_eq!(b.jurisdiction, "US-CA");
        assert!(b.rules.iter().all(|r| METRICS.contains(&r.metric.as_str())));
    }

    #[test]
    fn smoke_above_500_is_reachable() {
        // AQI caps at 500, so the respirator rule must be a concentration, or it never fires.
        let hours = metrics(&[raw(9, 600.0, 20.0), raw(10, 600.0, 20.0)]);
        let a = plan(&hours[1..], &us_ca());
        assert_eq!(a.len(), 1, "{a:?}");
        assert_eq!(a[0].rule, "smoke-respirators");
        assert!(a[0].text.contains("600.0"), "{}", a[0].text);
    }

    #[test]
    fn shift_plan_reports_first_crossing_and_clear_window() {
        // 07–17 shift; smoke arrives at 13:00, heat 82°F (27.8 °C) from 14:00.
        let raw: Vec<RawHour> = (0..24)
            .map(|h| raw(h, if h >= 13 { 80.0 } else { 5.0 }, if h >= 14 { 27.8 } else { 20.0 }))
            .collect();
        let p = shift_plan(&raw, "2026-10-24", 7, 17, &us_ca());
        let rules: Vec<(&str, &str)> = p.actions.iter().map(|a| (clock(&a.time), a.rule.as_str())).collect();
        // 13:00 NowCast at w = 0.5: (80 + 5·0.9995) / 1.9995 = 42.5 µg/m³, AQI 118: under 151, but
        // already over the BMS threshold of 101. 14:00: (80 + 40 + 5·0.4998) / 1.9995 = 61.2 → N95s.
        assert_eq!(rules, vec![("07:00", "plan"), ("14:00", "smoke-n95"), ("14:00", "heat-shade")]);
        assert_eq!(p.actions[0].text, "Heavy work in clear hours: 07:00–13:00");
        assert_eq!(p.hours.len(), 10);
        assert_eq!(p.bms.recirculate_at.as_deref(), Some("2026-10-24T13:00"));
        assert_eq!(p.bms.resume_at, None);
    }

    #[test]
    fn clean_day_gives_no_actions_and_normal_ventilation() {
        let raw: Vec<RawHour> = (0..24).map(|h| raw(h, 5.0, 18.0)).collect();
        let p = shift_plan(&raw, "2026-10-24", 7, 17, &us_ca());
        assert!(p.actions.is_empty());
        assert_eq!(p.bms.recirculate_at, None);
        assert!(p.bms.reason.contains("normal ventilation"));
    }
}
