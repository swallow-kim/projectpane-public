use super::{WatchReply, WatchRequest, contracts::WATCH_FRAME, native::Notification};
use crate::domain::{Fault, FaultCode};
use std::io::{self, BufRead, Read, Write};

#[cfg(all(test, not(miri)))]
mod tests;

#[derive(Default)]
struct Watches {
    generation: u32,
    handles: Vec<Option<Notification>>,
    paths: Vec<String>,
    slots: Vec<usize>,
}

impl Watches {
    fn request(&mut self, request: WatchRequest) -> Result<WatchReply, Fault> {
        request.validate()?;
        match request {
            WatchRequest::Replace { generation, paths } => {
                if generation <= self.generation {
                    return Err(Fault::new(FaultCode::StaleRequest));
                }
                let mut unique = Vec::new();
                let mut handles = Vec::new();
                let mut slots = Vec::new();
                let mut previous = std::mem::take(&mut self.handles);
                for path in paths {
                    let slot = match unique.iter().position(|value| value == &path) {
                        Some(slot) => slot,
                        None => {
                            let handle = match self.paths.iter().position(|value| value == &path) {
                                Some(old) => previous[old].take(),
                                // The unavailable mask reports every failed open; no retry or path logging.
                                None => Notification::open(&path).ok(),
                            };
                            handles.push(handle);
                            unique.push(path);
                            unique.len() - 1
                        }
                    };
                    slots.push(slot);
                }
                let watched =
                    u8::try_from(handles.iter().filter(|handle| handle.is_some()).count())
                        .map_err(|_| Fault::new(FaultCode::ResourceLimit))?;
                self.handles = handles;
                self.paths = unique;
                self.slots = slots;
                self.generation = generation;
                Ok(WatchReply::Configured {
                    generation,
                    watched,
                    unavailable: self.unavailable(),
                })
            }
            WatchRequest::Poll { generation } => {
                if generation != self.generation {
                    return Err(Fault::new(FaultCode::StaleRequest));
                }
                let changed = self
                    .handles
                    .iter_mut()
                    .map(|slot| {
                        let Some(handle) = slot else {
                            return false;
                        };
                        match handle.poll() {
                            Ok(changed) => changed,
                            Err(_) => {
                                *slot = None;
                                false
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                let mut mask = 0_u8;
                for (index, slot) in self.slots.iter().enumerate() {
                    if changed[*slot] {
                        mask |= 1_u8 << index;
                    }
                }
                Ok(WatchReply::Polled {
                    generation,
                    changed: mask,
                    unavailable: self.unavailable(),
                })
            }
        }
    }

    fn unavailable(&self) -> u8 {
        self.slots
            .iter()
            .enumerate()
            .fold(0, |mask, (index, slot)| {
                mask | if self.handles[*slot].is_none() {
                    1 << index
                } else {
                    0
                }
            })
    }
}

pub fn run() -> Result<(), Fault> {
    let input = io::stdin();
    let mut input = input.lock();
    let output = io::stdout();
    let mut output = output.lock();
    let mut watches = Watches::default();
    loop {
        let mut frame = Vec::new();
        (&mut input)
            .take(WATCH_FRAME + 1)
            .read_until(b'\n', &mut frame)?;
        if frame.is_empty() {
            return Ok(());
        }
        if u64::try_from(frame.len()).map_or(true, |length| length > WATCH_FRAME)
            || !frame.ends_with(b"\n")
        {
            return Err(Fault::new(FaultCode::ProtocolError));
        }
        let request = serde_json::from_slice::<WatchRequest>(&frame)
            .map_err(|_| Fault::new(FaultCode::ProtocolError))?;
        let reply = watches.request(request);
        serde_json::to_writer(&mut output, &reply)
            .map_err(|_| Fault::new(FaultCode::ProtocolError))?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
}
