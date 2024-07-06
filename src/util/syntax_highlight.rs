use clauser::{
    error::{Error, ErrorType},
    types::{CollectionType, Date, ObjectKey, Operator, TextPosition},
    value::{ValueOwned, ValueString},
    writer::{Writer, WriterOutput},
};

pub struct SyntaxHighlighter {}

impl SyntaxHighlighter {
    pub fn to_html(s: &mut String, code: &ValueOwned) -> Result<(), Error> {
        s.push_str("<div class=\"pd-highlight\">");
        let mut writer = HighlightedWriter::new(s);
        code.write(&mut writer)?;
        s.push_str("</div>");
        Ok(())
    }
}

#[derive(Debug)]
enum HighlightToken {
    Text,
    Identifier,
    String,
    Date,
    Bool,
    Number,
    Placeholder,
    Operator,
    Comment,
}

struct HighlightFrame {
    collection: CollectionType,
    same_line: bool,
}

struct HighlightedWriter<'out, T: WriterOutput> {
    output: &'out mut T,
    frames: Vec<HighlightFrame>,
    depth: i64,
    position: TextPosition,
    current_text: String,
    started: bool,
    has_written_token: bool,
}

impl<'out, T: WriterOutput> HighlightedWriter<'out, T> {
    fn write(&mut self, out: &str) -> Result<(), Error> {
        self.position.increment();
        self.output.push(out)
    }

    fn new_line(&mut self) -> Result<(), Error> {
        if !self.current_text.is_empty() {
            let next: String = self.current_text.drain(..).collect();
            self.current_text = String::new();
            self.write_span_for(HighlightToken::Text, &next)?;
        }

        self.write("<br/>")?;
        self.position.new_line();
        Ok(())
    }

    fn indent(&mut self) -> Result<(), Error> {
        if self.depth < 0 {
            return Err(Error::new_contextless(
                ErrorType::WriterError,
                self.position.index,
                "Tried to indent with negative depth!",
            ));
        }

        self.write_text(&" ".repeat((self.depth * 4) as usize))
    }

    fn write_span_for(&mut self, token: HighlightToken, out: &str) -> Result<(), Error> {
        let text = format!(
            "<span class=\"pd-token-{:?}\">{}</span>",
            token,
            handlebars::html_escape(out)
        );
        self.write(&text)?;
        Ok(())
    }

    fn write_text(&mut self, out: &str) -> Result<(), Error> {
        // we accumulate text until a new line or other token, so we don't emit tons of spans
        self.current_text.push_str(out);
        Ok(())
    }

    fn write_nontext(&mut self, token: HighlightToken, out: &str) -> Result<(), Error> {
        if !self.current_text.is_empty() {
            let next: String = self.current_text.drain(..).collect();
            self.current_text = String::new();
            self.write_span_for(HighlightToken::Text, &next)?;
        }

        self.has_written_token = true;
        self.write_span_for(token, out)
    }

    fn start_collection(
        &mut self,
        collection: CollectionType,
        same_line: bool,
    ) -> Result<(), Error> {
        if self.depth >= 0 {
            self.write_text("{ ")?;
        }

        if !same_line {
            self.depth += 1;
        }

        self.frames.push(HighlightFrame {
            collection,
            same_line,
        });
        Ok(())
    }

    fn end_collection(&mut self) -> Result<(), Error> {
        let frame = self.frames.pop().ok_or(Error::new_contextless(
            ErrorType::DepthMismatchError,
            self.position.index,
            "Tried to end collection that wasn't started!",
        ))?;

        if !frame.same_line {
            self.depth -= 1;

            self.new_line()?;
            if self.depth >= 0 {
                self.indent()?;
            }
        } else {
            self.write_text(" ")?;
        }

        if self.depth >= 0 {
            self.write_text("}")?;
        }

        self.flush_text()?;

        Ok(())
    }

    fn flush_text(&mut self) -> Result<(), Error> {
        if !self.current_text.is_empty() {
            let next: String = self.current_text.drain(..).collect();
            self.current_text = String::new();
            self.write_span_for(HighlightToken::Text, &next)?;
        }

        Ok(())
    }
}

impl<'out, T: WriterOutput> Writer<'out, T> for HighlightedWriter<'out, T> {
    fn new(output: &'out mut T) -> Self {
        HighlightedWriter {
            output,
            frames: Vec::new(),
            position: TextPosition::new(),
            current_text: String::new(),
            depth: -1,
            started: false,
            has_written_token: false,
        }
    }

    fn position(&self) -> TextPosition {
        self.position.clone()
    }

    fn begin_object(&mut self, _: Option<usize>) -> Result<(), Error> {
        self.start_collection(CollectionType::Object, false)
    }

    fn write_property<S: ValueString>(&mut self, key: &ObjectKey<S>) -> Result<(), Error> {
        if !self.started && self.has_written_token {
            // we've got stuff before this but we haven't yet written a property
            self.new_line()?;
        }

        let frame = self.frames.last().ok_or(Error::new_contextless(
            ErrorType::InvalidState,
            self.position.index,
            "Tried to write property with no collection!",
        ))?;

        if frame.collection != CollectionType::Object {
            return Err(Error::new_contextless(
                ErrorType::InvalidState,
                self.position.index,
                "Tried to write property from an array!",
            ));
        }

        if !frame.same_line && self.started {
            self.new_line()?;
            self.indent()?;
        }

        self.started = true;

        self.write_object_key(key)?;

        Ok(())
    }

    fn end_object(&mut self) -> Result<(), Error> {
        self.end_collection()
    }

    fn begin_array(&mut self, length: Option<usize>) -> Result<(), Error> {
        let same_line = length.map(|l| l < 4).unwrap_or(false);
        self.start_collection(CollectionType::Array, same_line)
    }

    fn end_array(&mut self) -> Result<(), Error> {
        self.end_collection()
    }

    fn write_direct(&mut self, string: &str) -> Result<(), Error> {
        self.write_text(string)
    }

    fn write_string(&mut self, string: &str) -> Result<(), Error> {
        self.write_nontext(HighlightToken::String, &format!("\"{}\"", string))
    }

    fn write_identifier(&mut self, string: &str) -> Result<(), Error> {
        self.write_nontext(HighlightToken::Identifier, string)
    }

    fn write_date(&mut self, date: &Date) -> Result<(), Error> {
        self.write_nontext(HighlightToken::Date, &date.to_string())
    }

    fn write_boolean(&mut self, b: bool) -> Result<(), Error> {
        self.write_nontext(
            HighlightToken::Bool,
            match b {
                true => "yes",
                false => "no",
            },
        )
    }

    fn write_placeholder(&mut self, placeholder: &str) -> Result<(), Error> {
        self.write_nontext(HighlightToken::Placeholder, &format!("<{}>", placeholder))
    }

    fn write_integer(&mut self, number: i64) -> Result<(), Error> {
        self.write_nontext(HighlightToken::Number, &number.to_string())
    }

    fn write_decimal(&mut self, number: f64) -> Result<(), Error> {
        self.write_nontext(HighlightToken::Number, &number.to_string())
    }

    fn write_operator(&mut self, operator: Operator) -> Result<(), Error> {
        let text = match operator {
            Operator::GreaterThan => ">",
            Operator::GreaterThanEq => ">=",
            Operator::LessThan => "<",
            Operator::LessThanEq => "<=",
        };

        self.write_nontext(HighlightToken::Operator, text)
    }

    fn write_comment(&mut self, comment: &str) -> Result<(), Error> {
        // don't write a new line if we're at the start of the file
        if self.has_written_token {
            self.new_line()?;
            self.indent()?;
        }
        self.write_nontext(HighlightToken::Comment, &format!("# {}", comment))
    }

    fn write_value(&mut self, val: &str) -> Result<(), Error> {
        self.write_direct(val)
    }
}
