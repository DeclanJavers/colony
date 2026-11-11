//! Game time. One tick is `minutes_per_tick` game minutes (1 in the base game). Tick 0 is the start
//! of spring, day 1, year 1.

use crate::content::Calendar;

impl Calendar {
    pub fn ticks_per_day(&self) -> u64 {
        (24 * 60 / self.minutes_per_tick) as u64
    }

    pub fn days_per_year(&self) -> u64 {
        (self.days_per_season as usize * self.seasons.len()) as u64
    }

    /// Days since the start, from 0.
    pub fn day_index(&self, tick: u64) -> u64 {
        tick / self.ticks_per_day()
    }

    pub fn year(&self, tick: u64) -> u64 {
        self.day_index(tick) / self.days_per_year() + 1
    }

    pub fn season_index(&self, tick: u64) -> usize {
        ((self.day_index(tick) % self.days_per_year()) / self.days_per_season as u64) as usize
    }

    pub fn season(&self, tick: u64) -> &str {
        &self.seasons[self.season_index(tick)]
    }

    /// Day within the season, from 1.
    pub fn day_of_season(&self, tick: u64) -> u64 {
        self.day_index(tick) % self.days_per_season as u64 + 1
    }

    pub fn minute_of_day(&self, tick: u64) -> u32 {
        ((tick % self.ticks_per_day()) * self.minutes_per_tick as u64) as u32
    }

    /// The tick at which a new game starts (spring, day 1, start_hour).
    pub fn start_tick(&self) -> u64 {
        (self.start_hour * 60 / self.minutes_per_tick) as u64
    }

    /// "spring, day 3, year 1, 06:00"
    pub fn describe(&self, tick: u64) -> String {
        let m = self.minute_of_day(tick);
        format!("{}, day {}, year {}, {:02}:{:02}", self.season(tick), self.day_of_season(tick), self.year(tick), m / 60, m % 60)
    }
}

#[cfg(test)]
mod tests {
    use crate::Content;

    #[test]
    fn calendar_counts() {
        let c = Content::base().calendar;
        let day = c.ticks_per_day();
        assert_eq!(day, 1440);
        assert_eq!(c.describe(c.start_tick()), "spring, day 1, year 1, 06:00");
        let year = day * c.days_per_year();
        assert_eq!(c.describe(year + day * 7 + 90), "summer, day 2, year 2, 01:30");
    }
}
