use std::iter;

use clauser::{
    error::{Error, ErrorType},
    types::{CollectionType, Date, ObjectKey, Operator, TextPosition},
    value::{Value, ValueOwned, ValueString},
    writer::{Writer, WriterOutput},
};

pub struct SyntaxHighlighter {}

impl SyntaxHighlighter {
    pub fn to_html(s: &mut String, code: &ValueOwned) -> Result<(), Error> {
        s.push_str("<div class=\"clhighlight\">");
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
}

struct HighlightFrame {
    collection: CollectionType,
    same_line: bool,
}

struct HighlightedWriter<'out, T: WriterOutput> {
    output: &'out mut T,
    frames: Vec<HighlightFrame>,
    depth: usize,
    position: TextPosition,
    current_text: String,
}

impl<'out, T: WriterOutput> HighlightedWriter<'out, T> {
    fn write(&mut self, out: &str) -> Result<(), Error> {
        self.position.increment();
        self.output.push(out)
    }

    fn new_line(&mut self) -> Result<(), Error> {
        self.write_text("<br/>")?;
        if !self.current_text.is_empty() {
            self.write_span_for(HighlightToken::Text, &self.current_text.clone())?;
            self.current_text = String::new();
        }

        self.position.new_line();
        Ok(())
    }

    fn indent(&mut self) -> Result<(), Error> {
        self.write_text(&iter::repeat('\t').take(self.depth).collect::<String>())
    }

    fn write_span_for(&mut self, token: HighlightToken, out: &str) -> Result<(), Error> {
        let text = format!(
            "<span class=\"cltoken-{:?}\">{}</span> ",
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
            self.write_span_for(HighlightToken::Text, out)?;
            self.current_text = String::new();
        }

        self.write_span_for(token, out)?;
        self.write_text(" ")
    }

    fn start_collection(
        &mut self,
        collection: CollectionType,
        same_line: bool,
    ) -> Result<(), Error> {
        self.write_text("{ ")?;
        if !same_line {
            self.new_line();
            self.indent();
            self.depth = self.depth + 1;
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

        if frame.same_line {
            self.write_text(" ")?;
        } else {
            self.indent()?;
        }

        self.write_text("}")?;
        if !frame.same_line {
            self.new_line()?;
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
            depth: 0,
        }
    }

    fn position(&self) -> TextPosition {
        self.position.clone()
    }

    fn begin_object(&mut self, length: Option<usize>) -> Result<(), Error> {
        let same_line = length.and_then(|l| Some(l < 2)).unwrap_or(false);
        self.start_collection(CollectionType::Object, same_line)
    }

    fn write_property<S: ValueString>(&mut self, key: &ObjectKey<S>) -> Result<(), Error> {
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

        if !frame.same_line {
            self.indent()?;
        }

        self.write_object_key(key)?;
        self.write_text(" ")?;

        Ok(())
    }

    fn end_object(&mut self) -> Result<(), Error> {
        self.end_collection()
    }

    fn begin_array(&mut self, length: Option<usize>) -> Result<(), Error> {
        let same_line = length.and_then(|l| Some(l < 4)).unwrap_or(false);
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
}
