use super::{invalid, Binding, Kind, Record};
use std::{collections::VecDeque, io};
/// One authorized generation. Replacement/resize requires a fresh receiver and binding.
pub struct Receiver {
    binding: Binding,
    configuration: Option<Record>,
    queue: VecDeque<Record>,
    last_sequence: u64,
    last_pts: Option<u64>,
    waiting: bool,
    keyframe_request: bool,
    closed: bool,
}
impl Receiver {
    pub fn new(binding: Binding) -> io::Result<Self> {
        binding.validate()?;
        Ok(Self {
            binding,
            configuration: None,
            queue: VecDeque::new(),
            last_sequence: 0,
            last_pts: None,
            waiting: true,
            keyframe_request: false,
            closed: false,
        })
    }
    pub fn accept(&mut self, record: Record) -> io::Result<()> {
        record.validate()?;
        if self.closed || record.binding != self.binding {
            return Err(invalid("Wrong/closed video generation"));
        }
        match record.kind {
            Kind::Configuration => {
                if self.configuration.is_some() {
                    return Err(invalid("Video reconfiguration requires a fresh generation"));
                }
                self.configuration = Some(record);
            }
            Kind::AccessUnit { idr } => {
                let config = self
                    .configuration
                    .as_ref()
                    .ok_or_else(|| invalid("Video configuration missing"))?;
                if (record.width, record.height) != (config.width, config.height)
                    || record.sequence <= self.last_sequence
                    || self.last_pts.is_some_and(|p| record.pts_us <= p)
                {
                    return Err(invalid("Video dimensions, sequence or time regressed"));
                }
                let gap = self.last_sequence.checked_add(1) != Some(record.sequence);
                self.last_sequence = record.sequence;
                self.last_pts = Some(record.pts_us);
                if gap || self.queue.len() == 2 {
                    self.queue.clear();
                    self.waiting = true;
                    self.keyframe_request = true;
                }
                if self.waiting && !idr {
                    self.keyframe_request = true;
                    return Ok(());
                }
                self.waiting = false;
                self.queue.push_back(record);
            }
        }
        Ok(())
    }
    pub fn pop(&mut self) -> Option<Record> {
        self.queue.pop_front()
    }
    pub fn configuration(&self) -> Option<&Record> {
        self.configuration.as_ref()
    }
    pub fn take_keyframe_request(&mut self) -> bool {
        std::mem::take(&mut self.keyframe_request)
    }
    pub fn clear(&mut self) {
        self.queue.clear();
        self.configuration = None;
        self.closed = true;
        self.keyframe_request = false;
    }
}
