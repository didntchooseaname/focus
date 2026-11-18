#[derive(Default)]
pub struct Parser {
    line: Vec<u8>,
    data: String,
    has_data: bool,
    skip_lf: bool,
    started: bool,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8], emit: &mut dyn FnMut(String)) {
        let mut bytes = bytes;
        if !self.started && !bytes.is_empty() {
            self.started = true;
            bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        }
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\n' => self.end_line(emit),
                b'\r' => {
                    self.end_line(emit);
                    self.skip_lf = true;
                }
                _ => self.line.push(byte),
            }
        }
    }

    pub fn finish(&mut self, _emit: &mut dyn FnMut(String)) {
        self.line.clear();
        self.data.clear();
        self.has_data = false;
    }

    fn end_line(&mut self, emit: &mut dyn FnMut(String)) {
        if self.line.is_empty() {
            self.dispatch(emit);
            return;
        }
        let line = std::mem::take(&mut self.line);
        let line = String::from_utf8_lossy(&line);
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (&*line, ""),
        };
        if field == "data" {
            if self.has_data {
                self.data.push('\n');
            }
            self.data.push_str(value);
            self.has_data = true;
        }
    }

    fn dispatch(&mut self, emit: &mut dyn FnMut(String)) {
        if self.has_data {
            self.has_data = false;
            emit(std::mem::take(&mut self.data));
        }
    }
}
