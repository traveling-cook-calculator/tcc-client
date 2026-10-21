//! Storage timestamps are naive UTC; users think in local time. All conversions live here.
//! None of them can panic on daylight-saving transitions (the old code `unwrap`ped
//! `Local.from_local_datetime(..).single()`).

use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone, Utc};

pub fn to_local(utc: NaiveDateTime) -> DateTime<Local> {
    Utc.from_utc_datetime(&utc).with_timezone(&Local)
}

/// Local wall-clock time of a UTC timestamp (what a date/time input should show).
pub fn local_wall_clock(utc: NaiveDateTime) -> NaiveDateTime {
    to_local(utc).naive_local()
}

/// `12.05.2026` – date only, in local time (cards, lists).
pub fn format_date(utc: NaiveDateTime) -> String {
    to_local(utc).format("%d.%m.%Y").to_string()
}

pub fn format_local(utc: NaiveDateTime) -> String {
    to_local(utc).format("%Y-%m-%d %H:%M").to_string()
}

/// Inverse of [`local_wall_clock`]. During the autumn clock change the earlier of the two
/// possible instants is used; a time that does not exist (spring gap) moves forward one hour.
pub fn local_to_utc(local: NaiveDateTime) -> Option<NaiveDateTime> {
    let resolve = |value: NaiveDateTime| {
        value
            .and_local_timezone(Local)
            .earliest()
            .map(|moment| moment.with_timezone(&Utc).naive_utc())
    };
    resolve(local).or_else(|| resolve(local + Duration::hours(1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn dates_are_formatted_day_first() {
        let utc = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap().and_hms_opt(12, 0, 0).unwrap();
        // Noon UTC is the same calendar day in every time zone from UTC-12 to UTC+11.
        assert_eq!(format_date(utc), "12.05.2026");
    }

    #[test]
    fn round_trip_keeps_the_instant() {
        let utc = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap().and_hms_opt(16, 30, 0).unwrap();
        assert_eq!(local_to_utc(local_wall_clock(utc)), Some(utc));
    }
}
