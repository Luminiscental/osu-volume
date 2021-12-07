use clap::{App, Arg};
use std::{
    error,
    fmt::{self, Display, Formatter},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
enum Error {
    InvalidInput(String),
    OnFileOpen(io::Error),
    NoSetFolder,
}

const TIMING_HEADER: &str = "[TimingPoints]";

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::InvalidInput(err) => write!(f, "Invalid input: {}", err),
            Self::OnFileOpen(err) => write!(f, "Could not open specified file: {}", err),
            Self::NoSetFolder => write!(
                f,
                "No set folder was found, try specifying a target file with --dest"
            ),
        }
    }
}

impl error::Error for Error {}

/// Enumerate the .osu files in the same folder as a given file
fn find_siblings<P>(path: P) -> Result<Vec<PathBuf>, Error>
where
    P: AsRef<Path>,
{
    let source_path = fs::canonicalize(path).map_err(Error::OnFileOpen)?;
    let set = fs::read_dir(source_path.parent().ok_or(Error::NoSetFolder)?)
        .map_err(Error::OnFileOpen)?;
    let mut siblings = Vec::new();
    for entry in set {
        let path = entry.map_err(Error::OnFileOpen)?.path();
        if path.extension().map(|s| s == "osu").unwrap_or(false) {
            siblings.push(path);
        }
    }
    Ok(siblings)
}

/// Milliseconds
type Time = usize;

/// Percent
type Volume = usize;

/// Parse the time and volume from a timing point
fn parse_point(line: &str) -> (Time, Volume) {
    let mut csv = line.split(',');
    let time = csv.next().unwrap().parse().unwrap();
    let volume = csv.nth(4).unwrap().parse().unwrap();
    (time, volume)
}

/// Overwrite the time and volume of a timing point
fn write_point(line: &str, point: (Time, Volume)) -> String {
    let mut commas = line.char_indices().filter(|c| c.1 == ',').map(|c| c.0);
    let after_time = commas.next().unwrap();
    let before_volume = commas.nth(3).unwrap();
    let after_volume = commas.next().unwrap();
    let time_string = point.0.to_string();
    let volume_string = point.1.to_string();
    [
        &time_string,
        &line[after_time..=before_volume],
        &volume_string,
        &line[after_volume..],
    ]
    .concat()
}

/// Split into (before_timing, timing, after_timing) where timing contains the
/// timing points with no preceding or succeeding newlines
fn extract_timing(source: &str) -> (&str, &str, &str) {
    let start = source.find(TIMING_HEADER).unwrap() + TIMING_HEADER.len() + 2;
    let end = start + source[start..].find("\r\n\r\n").unwrap();
    (&source[..start], &source[start..end], &source[end..])
}

/// Convert an uninherited line to an inherited line with default effects
fn make_inherited(line: &str) -> String {
    let mut csv: Vec<_> = line.split(',').collect();
    if csv[6] == "1" {
        csv[1] = "-100";
        csv[6] = "0";
    }
    csv.join(",")
}

/// Check if two timing points are the same ignoring their timestamps
fn same_after_time(line1: &mut String, line2: &mut String) -> bool {
    let idx1 = line1.find(',').unwrap_or(0);
    let idx2 = line2.find(',').unwrap_or(0);
    line1[idx1..] == line2[idx2..]
}

/// Check if two timing points have the same volume
fn same_volume(
    point1: &mut (Time, Volume),
    point2: &mut (Time, Volume),
) -> bool {
    point1.1 == point2.1
}

struct VolumeCurve {
    points: Vec<(Time, Volume)>,
}

impl VolumeCurve {
    fn parse(source: &str, mute_threshold: Volume) -> Self {
        let (_, timing, _) = extract_timing(source);
        let mut points: Vec<_> = timing
            .lines()
            .map(parse_point)
            .filter(|point| point.1 > mute_threshold)
            .collect();
        points.dedup_by(same_volume);
        Self { points }
    }

    fn load<P>(source: P, mute_threshold: Volume) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let source = fs::read_to_string(source).map_err(Error::OnFileOpen)?;
        Ok(Self::parse(&source, mute_threshold))
    }

    fn apply(&self, source: &str, mute_threshold: Volume) -> String {
        if self.points.is_empty() {
            return source.to_owned();
        }
        let (before_timing, timing, after_timing) = extract_timing(source);
        let mut new_timing = Vec::new();
        let mut write_idx = 0;
        let mut current_volume = 100;
        let mut last_line = "";
        for line in timing.lines() {
            let old_point = parse_point(line);
            while write_idx < self.points.len()
                && self.points[write_idx].0 < old_point.0
            {
                if !last_line.is_empty() {
                    new_timing.push(write_point(
                        &make_inherited(last_line),
                        self.points[write_idx],
                    ));
                }
                current_volume = self.points[write_idx].1;
                write_idx += 1;
            }
            if write_idx < self.points.len()
                && self.points[write_idx].0 == old_point.0
            {
                new_timing.push(write_point(line, self.points[write_idx]));
                current_volume = self.points[write_idx].1;
                write_idx += 1;
            } else {
                let new_volume = if old_point.1 > mute_threshold {
                    current_volume
                } else {
                    old_point.1
                };
                new_timing.push(write_point(
                    &make_inherited(line),
                    (old_point.0, new_volume),
                ));
            }
            last_line = line;
        }
        while write_idx < self.points.len() {
            new_timing.push(write_point(
                &make_inherited(last_line),
                self.points[write_idx],
            ));
            write_idx += 1;
        }
        new_timing.dedup_by(same_after_time);
        let new_timing = new_timing.join("\r\n");
        [before_timing, &new_timing, after_timing].concat()
    }

    fn write<P>(&self, dest: P, mute_threshold: Volume) -> Result<(), Error>
    where
        P: AsRef<Path>,
    {
        let contents = fs::read_to_string(&dest).map_err(Error::OnFileOpen)?;
        fs::write(dest, self.apply(&contents, mute_threshold))
            .map_err(Error::OnFileOpen)
    }
}

fn main() -> Result<(), Error> {
    let matches = App::new("osu-volume")
        .version("1.0")
        .author("Luminiscental <luminiscental01@gmail.com>")
        .about("Copy the volume curve from one difficulty of an osu map to other difficulties in the set.")
        .arg(Arg::with_name("source").help("The .osu file to copy the volume curve from.").required(true))
        .arg(Arg::with_name("dest").long("dest").takes_value(true).help("Optionally specify a specific .osu file to copy the volume curve to. If not present this defaults to all other diffs in the beatmapset."))
        .arg(Arg::with_name("mute_threshold").long("mute_threshold").takes_value(true).help("Ignore greenlines with volumes less than or equal to this (treat them as muting sliderends).").default_value("5"))
        .get_matches();
    let source = PathBuf::from(matches.value_of("source").unwrap());
    let mute_threshold = matches
        .value_of("mute_threshold")
        .unwrap()
        .parse()
        .map_err(|err| {
            Error::InvalidInput(format!(
                "Expected integer for volume threshold: {}",
                err
            ))
        })?;
    let targets = if let Some(dest) = matches.value_of("dest") {
        vec![PathBuf::from(dest)]
    } else {
        find_siblings(&source)?
    };
    let volume_curve = VolumeCurve::load(source, mute_threshold)?;
    for target in targets {
        volume_curve.write(target, mute_threshold)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_point_works() {
        assert_eq!(parse_point("95,517.241379310345,4,2,1,50,1,0"), (95, 50));
    }

    #[test]
    fn write_point_works() {
        assert_eq!(
            write_point("15,326.086956521739,4,2,0,30,1,0", (10, 70)),
            "10,326.086956521739,4,2,0,70,1,0"
        );
    }

    #[test]
    fn volume_curve_parses() {
        let source = include_str!("testdiff.in");
        let volume_curve = VolumeCurve::parse(source, 5);
        assert_eq!(
            volume_curve.points,
            vec![
                (15, 30),
                (1319, 20),
                (1563, 15),
                (1808, 10),
                (2053, 50),
                (2623, 20)
            ]
        );
    }

    #[test]
    fn self_volume_curve_identity() {
        let source = include_str!("testdiff.in");
        let application = VolumeCurve::parse(source, 5).apply(&source, 5);
        assert_eq!(application, source);
    }

    #[test]
    fn empty_volume_curve_identity() {
        let source = include_str!("testdiff.in");
        assert_eq!(
            VolumeCurve { points: Vec::new() }.apply(&source, 5),
            source
        );
    }

    #[test]
    fn volume_curve_idempotent() {
        let curve = VolumeCurve {
            points: vec![(1, 20), (998, 80), (3011, 45)],
        };
        let source = include_str!("testdiff.in");
        let once = curve.apply(&source, 5);
        let twice = curve.apply(&once, 5);
        assert_eq!(once, twice);
    }

    #[test]
    fn volume_curve_applies() {
        let curve = VolumeCurve {
            points: vec![
                (5, 100),
                (8, 10),
                (15, 20),
                (101, 30),
                (1400, 20),
                (1563, 15),
                (2053, 100),
                (2417, 30),
                (3000, 50),
            ],
        };
        let source = include_str!("testdiff.in");
        let expected = include_str!("testdiff_output.in");
        assert_eq!(curve.apply(&source, 5), expected);
    }
}
