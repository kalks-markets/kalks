//! Timeframes and broker-standard bar alignment.
//!
//! M1–H1 bars start on exact UTC minute/hour boundaries (identical on every platform).
//! H4, D1, W1 and MN are cut at New York close, like MT4/MT5 brokers: server time is GMT+3 while
//! US daylight saving is active and GMT+2 otherwise; weeks start Sunday 00:00 server time. A bar opens at the UTC
//! instant its server-time boundary had then (so on the switch day the server day / week / month is one bar, an hour
//! shorter or longer, never two bars or a bar opening before the one it follows).

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tf {
    M1,
    M5,
    M15,
    M30,
    H1,
    H4,
    D1,
    W1,
    MN,
}

impl Tf {
    pub const ALL: [Tf; 9] = [Tf::M1, Tf::M5, Tf::M15, Tf::M30, Tf::H1, Tf::H4, Tf::D1, Tf::W1, Tf::MN];

    /// Stored in the `candles.tf` column (minutes).
    pub fn minutes(self) -> i32 {
        match self {
            Tf::M1 => 1,
            Tf::M5 => 5,
            Tf::M15 => 15,
            Tf::M30 => 30,
            Tf::H1 => 60,
            Tf::H4 => 240,
            Tf::D1 => 1440,
            Tf::W1 => 10080,
            Tf::MN => 43200,
        }
    }

    pub fn from_minutes(m: i32) -> Option<Tf> {
        Tf::ALL.into_iter().find(|t| t.minutes() == m)
    }

    pub fn name(self) -> &'static str {
        match self {
            Tf::M1 => "M1",
            Tf::M5 => "M5",
            Tf::M15 => "M15",
            Tf::M30 => "M30",
            Tf::H1 => "H1",
            Tf::H4 => "H4",
            Tf::D1 => "D1",
            Tf::W1 => "W1",
            Tf::MN => "MN",
        }
    }

    pub fn parse(s: &str) -> Option<Tf> {
        Tf::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(s))
    }

    /// Infoway `klineType` for timeframes we backfill directly from the provider.
    pub fn provider_kline(self) -> Option<i32> {
        match self {
            Tf::M1 => Some(1),
            Tf::M5 => Some(2),
            Tf::M15 => Some(3),
            Tf::M30 => Some(4),
            Tf::H1 => Some(5),
            Tf::D1 => Some(8),
            _ => None,
        }
    }

    /// Open time (UTC) of the bar that contains `ts`.
    pub fn bucket(self, ts: DateTime<Utc>) -> DateTime<Utc> {
        let secs = ts.timestamp();
        match self {
            Tf::M1 | Tf::M5 | Tf::M15 | Tf::M30 | Tf::H1 => {
                let d = self.minutes() as i64 * 60;
                Utc.timestamp_opt(secs.div_euclid(d) * d, 0).unwrap()
            }
            Tf::H4 | Tf::D1 => {
                let d = self.minutes() as i64 * 60;
                let local = secs + server_offset_secs(ts);
                server_to_utc(local.div_euclid(d) * d)
            }
            Tf::W1 => {
                let local = Utc.timestamp_opt(secs + server_offset_secs(ts), 0).unwrap().date_naive();
                let back = local.weekday().num_days_from_sunday() as i64;
                let sunday = local - Duration::days(back);
                server_to_utc(midnight(sunday).timestamp())
            }
            Tf::MN => {
                let local = Utc.timestamp_opt(secs + server_offset_secs(ts), 0).unwrap().date_naive();
                let first = NaiveDate::from_ymd_opt(local.year(), local.month(), 1).unwrap();
                server_to_utc(midnight(first).timestamp())
            }
        }
    }
}

/// A server-time wall clock reading (seconds) → the UTC instant it names, with the offset in force at that instant.
/// In the hour the clocks go back it names two instants: the first (still GMT+3).
fn server_to_utc(local: i64) -> DateTime<Utc> {
    let summer = local - 3 * 3600;
    let utc = if server_offset_secs(Utc.timestamp_opt(summer, 0).unwrap()) == 3 * 3600 { summer } else { local - 2 * 3600 };
    Utc.timestamp_opt(utc, 0).unwrap()
}

fn midnight(d: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
}

/// n-th `weekday` of a month (1-based).
fn nth_weekday(year: i32, month: u32, weekday: Weekday, n: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let shift = (7 + weekday.num_days_from_sunday() as i64 - first.weekday().num_days_from_sunday() as i64) % 7;
    first + Duration::days(shift + 7 * (n as i64 - 1))
}

/// Server-time offset (seconds east of UTC): +3h during US DST (NY close = 00:00), +2h otherwise.
/// US DST: second Sunday of March 07:00 UTC → first Sunday of November 06:00 UTC.
pub fn server_offset_secs(ts: DateTime<Utc>) -> i64 {
    let y = ts.year();
    let start = midnight(nth_weekday(y, 3, Weekday::Sun, 2)) + Duration::hours(7);
    let end = midnight(nth_weekday(y, 11, Weekday::Sun, 1)) + Duration::hours(6);
    if ts >= start && ts < end {
        3 * 3600
    } else {
        2 * 3600
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn intraday_buckets_are_utc_aligned() {
        assert_eq!(Tf::M15.bucket(t("2026-09-25T09:14:59Z")), t("2026-09-25T09:00:00Z"));
        assert_eq!(Tf::H1.bucket(t("2026-09-25T09:59:59Z")), t("2026-09-25T09:00:00Z"));
    }

    #[test]
    fn daily_bar_starts_at_ny_close() {
        // September (US DST): server GMT+3 → day starts 21:00 UTC the previous day
        assert_eq!(Tf::D1.bucket(t("2026-09-25T09:00:00Z")), t("2026-09-24T21:00:00Z"));
        assert_eq!(Tf::D1.bucket(t("2026-09-24T21:00:00Z")), t("2026-09-24T21:00:00Z"));
        // January (no DST): server GMT+2 → day starts 22:00 UTC
        assert_eq!(Tf::D1.bucket(t("2026-01-15T12:00:00Z")), t("2026-01-14T22:00:00Z"));
    }

    #[test]
    fn h4_week_month() {
        assert_eq!(Tf::H4.bucket(t("2026-09-25T09:30:00Z")), t("2026-09-25T09:00:00Z")); // 12:00 server
        // 2026-09-25 is a Friday; week starts Sunday 2026-09-20 00:00 server = 09-19 21:00 UTC
        assert_eq!(Tf::W1.bucket(t("2026-09-25T09:30:00Z")), t("2026-09-19T21:00:00Z"));
        assert_eq!(Tf::MN.bucket(t("2026-09-25T09:30:00Z")), t("2026-08-31T21:00:00Z"));
    }

    /// Crypto trades through both US DST switches (Sunday 2026-03-08 07:00 UTC and Sunday 2026-11-01 06:00 UTC, the
    /// first of a month). Every tick of the switch day must land in one bar per server day / week / month / 4 hours,
    /// with bar open times that never go back as time goes on (no second November bar, no H4 bar opening before the
    /// one it follows).
    #[test]
    fn buckets_stay_whole_and_in_order_across_the_dst_switches() {
        for (from, to) in [("2026-03-07T12:00:00Z", "2026-03-09T12:00:00Z"), ("2026-10-31T12:00:00Z", "2026-11-02T12:00:00Z")] {
            for tf in Tf::ALL {
                let mut ts = t(from);
                let mut prev = tf.bucket(ts);
                while ts < t(to) {
                    let b = tf.bucket(ts);
                    assert!(b <= ts, "{tf:?} {ts}: bar opens after the tick ({b})");
                    assert!(b >= prev, "{tf:?} {ts}: bar {b} opens before the previous one {prev}");
                    assert_eq!(tf.bucket(b), b, "{tf:?} {ts}: the bar's open time is in another bar");
                    prev = b;
                    ts += Duration::minutes(1);
                }
            }
        }
        // the server day, week and month of the autumn switch are one bar each, 25 hours longer by one hour
        assert_eq!(Tf::MN.bucket(t("2026-11-01T05:59:00Z")), t("2026-10-31T21:00:00Z"));
        assert_eq!(Tf::MN.bucket(t("2026-11-01T06:00:00Z")), t("2026-10-31T21:00:00Z"));
        assert_eq!(Tf::W1.bucket(t("2026-11-01T12:00:00Z")), t("2026-10-31T21:00:00Z"));
        assert_eq!(Tf::D1.bucket(t("2026-11-01T21:59:00Z")), t("2026-10-31T21:00:00Z"));
        assert_eq!(Tf::D1.bucket(t("2026-11-01T22:00:00Z")), t("2026-11-01T22:00:00Z"));
        // H4 after the spring switch: 06:00 UTC (08:00 server, GMT+2) runs to 09:00 UTC (12:00 server, GMT+3)
        assert_eq!(Tf::H4.bucket(t("2026-03-08T07:30:00Z")), t("2026-03-08T06:00:00Z"));
        assert_eq!(Tf::H4.bucket(t("2026-03-08T09:00:00Z")), t("2026-03-08T09:00:00Z"));
        assert_eq!(Tf::D1.bucket(t("2026-03-08T12:00:00Z")), t("2026-03-07T22:00:00Z"));
    }

    #[test]
    fn dst_switch_dates() {
        assert_eq!(server_offset_secs(t("2026-03-08T06:59:00Z")), 7200);
        assert_eq!(server_offset_secs(t("2026-03-08T07:00:00Z")), 10800);
        assert_eq!(server_offset_secs(t("2026-11-01T05:59:00Z")), 10800);
        assert_eq!(server_offset_secs(t("2026-11-01T06:00:00Z")), 7200);
    }
}
