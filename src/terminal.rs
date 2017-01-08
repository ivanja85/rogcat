// Copyright © 2016 Felix Obenhuber
// This program is free software. It comes without any warranty, to the extent
// permitted by applicable law. You can redistribute it and/or modify it under
// the terms of the Do What The Fuck You Want To Public License, Version 2, as
// published by Sam Hocevar. See the COPYING file for more details.

use regex::Regex;
use std::cmp::max;
use std::collections::HashMap;
use std::io::Write;
use std::io::stdout;
use super::node::Node;
use super::record::{Level, Record};
use term_painter::Attr::*;
use term_painter::{Color, ToStyle};
use terminal_size::{Width, Height, terminal_size};

const DIMM_COLOR: Color = Color::Custom(243);

pub struct Terminal<'a> {
    beginning_of: Regex,
    color: bool,
    date_format: (&'a str, usize),
    full_tag: bool,
    process_width: usize,
    tag_timestamps: HashMap<String, ::time::Tm>,
    tag_width: usize,
    thread_width: usize,
    vovels: Regex,
    time_diff: bool,
    diff_width: usize,
}

impl<'a> Terminal<'a> {
    /// Filter some unreadable (on dark background) or nasty colors
    fn hashed_color(item: &str) -> Color {
        match item.bytes().fold(42u16, |c, x| c ^ x as u16) {
            c @ 0...1 => Color::Custom(c + 2),
            c @ 16...21 => Color::Custom(c + 6),
            c @ 52...55 | c @ 126...129 => Color::Custom(c + 4),
            c @ 163...165 | c @ 200...201 => Color::Custom(c + 3),
            c @ 207 => Color::Custom(c + 1),
            c @ 232...240 => Color::Custom(c + 9),
            c @ _ => Color::Custom(c),
        }
    }

    // TODO
    // Rework this to use a more column based approach!
    fn print_record(&mut self, record: Record) {
        let (timestamp, diff) = if let Some(ts) = record.timestamp {
            let timestamp = match ::time::strftime(self.date_format.0, &ts) {
                Ok(t) => {
                    t.chars()
                        .take(self.date_format.1)
                        .collect::<String>()
                }
                Err(_) => "".to_owned(),
            };

            let diff = if self.time_diff {
                if let Some(t) = self.tag_timestamps.get_mut(&record.tag) {
                    let diff = (ts - *t).num_milliseconds() as f32 / 1000.0;
                    let diff = format!("{:>.3}", diff);
                    let diff = if diff.chars().count() <= self.diff_width {
                        diff
                    } else {
                        "-.---".to_owned()
                    };

                    diff
                } else {
                    "".to_owned()
                }
            } else {
                "".to_owned()
            };

            (timestamp, diff)
        } else {
            ("".to_owned(), "".to_owned())
        };

        let tag = {
            let t = if self.beginning_of.is_match(&record.message) {
                // Print horizontal line if temrinal width is detectable
                if let Some((Width(width), Height(_))) = terminal_size() {
                    println!("{}", (0..width).map(|_| "─").collect::<String>());
                }
                // "beginnig of" messages never have a tag
                &record.message
            } else {
                &record.tag
            };

            if self.full_tag {
                format!("{:>width$}", t, width = self.tag_width)
            } else if t.chars().count() > self.tag_width {
                let mut t = self.vovels.replace_all(t, "");
                if t.chars().count() > self.tag_width {
                    t.truncate(self.tag_width);
                }
                format!("{:>width$}", t, width = self.tag_width)
            } else {
                format!("{:>width$}", t, width = self.tag_width)
            }
        };

        self.process_width = max(self.process_width, record.process.chars().count());
        let pid = format!("{:<width$}", record.process, width = self.process_width);
        let tid = if record.thread.is_empty() {
            "".to_owned()
        } else {
            self.thread_width = max(self.thread_width, record.thread.chars().count());
            format!(" {:>width$}", record.thread, width = self.thread_width)
        };

        let level = format!(" {} ", record.level);
        let level_color = match record.level {
            Level::Trace | Level::Debug | Level::None => DIMM_COLOR,
            Level::Info => Color::Green,
            Level::Warn => Color::Yellow,
            Level::Error | Level::Fatal | Level::Assert => Color::Red,
        };

        let color = self.color;
        let tag_width = self.tag_width;
        let diff_width = self.diff_width;
        let timestamp_width = self.date_format.1;
        let print_msg = |chunk: &str, sign: &str| {
            if color {
                println!("{:<timestamp_width$} {:>diff_width$} {:>tag_width$} ({}{}) {} {} {}",
                         DIMM_COLOR.paint(&timestamp),
                         DIMM_COLOR.paint(&diff),
                         Self::hashed_color(&tag).paint(&tag),
                         Self::hashed_color(&pid).paint(&pid),
                         Self::hashed_color(&tid).paint(&tid),
                         Plain.bg(level_color).fg(Color::Black).paint(&level),
                         level_color.paint(sign),
                         level_color.paint(chunk),
                         timestamp_width = timestamp_width,
                         diff_width = diff_width,
                         tag_width = tag_width);

            } else {
                println!("{:<timestamp_width$} {:>diff_width$} {:>tag_width$} ({}{}) {} {} {}",
                         timestamp,
                         diff,
                         tag,
                         pid,
                         tid,
                         level,
                         sign,
                         chunk,
                         timestamp_width = timestamp_width,
                         diff_width = diff_width,
                         tag_width = tag_width);
            }
        };

        if let Some((Width(width), Height(_))) = terminal_size() {
            let preamble_width = timestamp_width + 1 + self.diff_width + 1 + tag_width + 1 +
                                 2 * self.process_width + 2 + 1 +
                                 8;
            let record_len = record.message.chars().count();
            let columns = width as usize;
            if (preamble_width + record_len) > columns {
                let mut m = record.message.clone();
                // TODO: Refactor this!
                while !m.is_empty() {
                    let chars_left = m.chars().count();
                    let (chunk_width, sign) = if chars_left == record_len {
                        (columns - preamble_width, "┌")
                    } else if chars_left <= (columns - preamble_width) {
                        (chars_left, "└")
                    } else {
                        (columns - preamble_width, "├")
                    };

                    let chunk: String = m.chars().take(chunk_width).collect();
                    m = m.chars().skip(chunk_width).collect();
                    if self.color {
                        let c = level_color.paint(chunk).to_string();
                        print_msg(&c, sign)
                    } else {
                        print_msg(&chunk, sign)
                    }
                }
            } else {
                print_msg(&record.message, " ");
            }
        } else {
            print_msg(&record.message, " ");
        };

        if let Some(ts) = record.timestamp {
            if self.time_diff && !record.tag.is_empty() {
                self.tag_timestamps.insert(record.tag.clone(), ts);
            }
        }

        stdout().flush().unwrap();
    }
}

impl<'a> Node<Record, ()> for Terminal<'a> {
    fn new(_: ()) -> Result<Box<Self>, String> {
        Ok(Box::new(Terminal {
            beginning_of: Regex::new(r"--------- beginning of.*").unwrap(),
            color: true, // ! args.is_present("NO-COLOR"),
            date_format: if true {
                ("%m-%d %H:%M:%S.%f", 18)
            } else {
                ("%H:%M:%S.%f", 12)
            },
            full_tag: false, // args.is_present("NO-TAG-SHORTENING"),
            process_width: 0,
            tag_timestamps: HashMap::new(),
            vovels: Regex::new(r"a|e|i|o|u").unwrap(),

            tag_width: 20,
            thread_width: 0,
            diff_width: 8,
            time_diff: true, // !args.is_present("NO-TIME-DIFF"),
        }))
    }

    fn message(&mut self, record: Record) -> Result<Option<Record>, String> {
        self.print_record(record);
        Ok(None)
    }
}
