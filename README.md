# osu-volume

## What?

This is a commandline utility I made for copying the volume profiler between
difficulties of an [osu!](https://osu.ppy.sh/home) beatmap. Basic usage looks
like:

```sh
# copies volume from [Use This Diff] to all other diffs in the mapset
$ osu-volume "my_osu_folder/Songs/my_mapset/my_mapset [Use This Diff].osu"
```

## How?

To install and use this, either download an executable from the
[releases](https://github.com/Luminiscental/osu-volume/releases) section or
build from source, which requires [cargo](https://doc.rust-lang.org/cargo/) from
the rust toolchain. Compilation from source is as simple as running
`cargo build` after cloning / downloading the repository, which will create an
executable in a folder called `target`. For more information about using the
command you can run `osu-volume --help`.

## Why?

I made this more for a quick use-case out of interest rather than practicality,
and being a command-line tool this isn't the most user-friendly. For similar
functionality see OliBomby's [mapping tools](https://mappingtools.seira.moe/)
which boasts a wide range of mapping utilities packaged into a user-friendly
GUI.
