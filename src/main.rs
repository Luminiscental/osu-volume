use clap::{App, Arg};
use std::{
    error,
    fmt::{self, Display, Formatter},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
enum Error {
    FileOpenError(io::Error),
    NoSetFolder,
}

const TIMING_HEADER: &str = "[TimingPoints]";
const DEFAULT_TIMING_LINE: &str = "0,-100,4,2,1,100,1,0";

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::FileOpenError(err) => write!(f, "Could not open specified file: {}", err),
            Self::NoSetFolder => write!(
                f,
                "No set folder was found, try specifying a target file with --dest"
            ),
        }
    }
}

impl error::Error for Error {}

/// Enumerate the other .osu files in a beatmapset
fn find_siblings<P>(source: P) -> Result<Vec<PathBuf>, Error>
where
    P: AsRef<Path>,
{
    let source_path = fs::canonicalize(source).map_err(Error::FileOpenError)?;
    let set = fs::read_dir(source_path.parent().ok_or(Error::NoSetFolder)?)
        .map_err(Error::FileOpenError)?;
    let mut siblings = Vec::new();
    for entry in set {
        let path = entry.map_err(Error::FileOpenError)?.path();
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

/// Converts red lines to 1xSV green lines
fn make_inherited(line: &str) -> String {
    let mut csv: Vec<_> = line.split(',').collect();
    if !csv[1].starts_with('-') {
        csv[1] = "-100";
    }
    csv.join(",")
}

struct VolumeCurve {
    points: Vec<(Time, Volume)>,
}

impl VolumeCurve {
    fn parse(source: &str) -> Self {
        let (_, timing, _) = extract_timing(source);
        let mut points: Vec<_> = timing.lines().map(parse_point).collect();
        points.dedup();
        Self { points }
    }

    fn load<P>(source: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let source =
            fs::read_to_string(source).map_err(Error::FileOpenError)?;
        Ok(Self::parse(&source))
    }

    fn apply(&self, source: &str) -> String {
        if self.points.is_empty() {
            return source.to_owned();
        }
        let (before_timing, timing, after_timing) = extract_timing(source);
        let mut new_timing = Vec::new();
        let mut write_idx = 0;
        let mut current_volume = 100;
        // TODO: it's probably a better behaviour choice to check and not insert
        // anything until we've hit the first red line rather than having this
        // default get inserted when the volume curve wants to do stuff before
        // the first red line
        let mut last_line = DEFAULT_TIMING_LINE;
        for line in timing.lines() {
            let old_point = parse_point(line);
            while write_idx < self.points.len()
                && self.points[write_idx].0 < old_point.0
            {
                new_timing.push(write_point(
                    &make_inherited(last_line),
                    self.points[write_idx],
                ));
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
                new_timing.push(write_point(
                    &make_inherited(line),
                    (old_point.0, current_volume),
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
        new_timing.dedup();
        let new_timing = new_timing.join("\r\n");
        [before_timing, &new_timing, after_timing].concat()
    }

    fn write<P>(&self, dest: P) -> Result<(), Error>
    where
        P: AsRef<Path>,
    {
        let contents =
            fs::read_to_string(&dest).map_err(Error::FileOpenError)?;
        fs::write(dest, self.apply(&contents)).map_err(Error::FileOpenError)
    }
}

fn main() -> Result<(), Error> {
    let matches = App::new("osu-volume")
        .version("1.0")
        .author("Luminiscental <luminiscental01@gmail.com>")
        .about("Copy the volume curve from one difficulty of an osu map to other difficulties in the set.")
        .arg(Arg::with_name("source").help("The .osu file to copy the volume curve from.").required(true))
        .arg(Arg::with_name("dest").help("Optionally specify a specific .osu file to copy the volume curve to. If not present this defaults to all other diffs in the beatmapset."))
        .get_matches();
    let source = PathBuf::from(matches.value_of("source").unwrap());
    let targets = if let Some(dest) = matches.value_of("dest") {
        vec![PathBuf::from(dest)]
    } else {
        find_siblings(&source)?
    };
    let volume_curve = VolumeCurve::load(source)?;
    for target in targets {
        volume_curve.write(target)?;
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
        let volume_curve = VolumeCurve::parse(source);
        assert_eq!(
            volume_curve.points,
            vec![
                (15, 30),
                (1319, 20),
                (1563, 15),
                (1808, 10),
                (2053, 5),
                (2623, 20)
            ]
        );
    }

    #[test]
    fn self_volume_curve_identity() {
        let source = include_str!("testdiff.in");
        let application = VolumeCurve::parse(source).apply(&source);
        assert_eq!(application, source);
    }

    #[test]
    fn empty_volume_curve_identity() {
        let source = include_str!("testdiff.in");
        assert_eq!(VolumeCurve { points: Vec::new() }.apply(&source), source);
    }

    #[test]
    fn volume_curve_idempotent() {
        let curve = VolumeCurve {
            points: vec![(1, 20), (998, 80), (3011, 45)],
        };
        let source = include_str!("testdiff.in");
        let once = curve.apply(&source);
        let twice = curve.apply(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn volume_curve_applies() {
        let curve = VolumeCurve {
            points: vec![
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
        assert_eq!(curve.apply(&source), expected);
    }
}
