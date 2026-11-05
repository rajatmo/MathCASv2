//! Cell styles (design 2.1): each style's grouping rule, default attributes and look, and its file kind.
use mc_nbfile::Kind;

/// A cell style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    /// the notebook's title (`Alt+1`)
    Title,
    /// `Alt+2`
    Subtitle,
    /// a third title level
    Subsubtitle,
    /// `Alt+3`
    Chapter,
    /// below a chapter
    Subchapter,
    /// `Alt+4`
    Section,
    /// `Alt+5`
    Subsection,
    /// `Alt+6`
    Subsubsection,
    /// prose (`Alt+7`)
    Text,
    /// initialization code (`Alt+8`)
    Code,
    /// evaluatable input, the style of new cells (`Alt+9`)
    Input,
    /// a bulleted item
    Item,
    /// a paragraph continuing an item
    ItemParagraph,
    /// a second-level item
    Subitem,
    /// a paragraph continuing a subitem
    SubitemParagraph,
    /// a third-level item
    Subsubitem,
    /// a numbered item
    ItemNumbered,
    /// a numbered second-level item
    SubitemNumbered,
    /// a numbered third-level item
    SubsubitemNumbered,
    /// a displayed formula
    DisplayFormula,
    /// program text that is not evaluated
    Program,
    /// a value made by evaluation
    Output,
    /// `print` output made by evaluation
    Print,
    /// an error or warning made by evaluation
    Message,
}

/// How a style takes part in automatic grouping (design 2.3). The numbers are levels: smaller is more important.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupRule {
    /// Title (0) starts a group running to the next Title; Subtitle (10) and Subsubtitle (20) are plain members
    Title(u8),
    /// starts a group that runs until a title or a section of the same or a smaller level
    Section(u8),
    /// starts a group of following items of higher level and input groups; ended by normal cells
    Item(u8),
    /// starts a group with the output cells right after it
    Input,
    /// joins the input group before it
    Output,
    /// starts no group; ends item groups
    Normal,
}

/// What Return does in a style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnKey {
    /// a line break in the cell
    NewLine,
    /// a new cell of the same style after this one
    NewCell,
    /// editing a generated output duplicates it as an input (design 2.8)
    Duplicate,
    /// the cell cannot be edited
    ReadOnly,
}

/// Which font family a style is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontRole {
    /// the code font (Input, Code, Output, Print, Program)
    Code,
    /// the prose font (titles, sections, text, items)
    Prose,
    /// the interface font (messages)
    Ui,
}

/// The look and behaviour of a style (design 2.1); margins in points at 100 % magnification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleInfo {
    /// the display name
    pub name: &'static str,
    /// the `Alt+digit` key, if any
    pub key: Option<u8>,
    /// the grouping rule
    pub group: GroupRule,
    /// Shift+Enter sends the cell to the kernel
    pub evaluatable: bool,
    /// made by evaluation
    pub generated: bool,
    /// whether an `In[n]:=` / `Out[n]=` label is shown
    pub label: bool,
    /// the font size in points
    pub size: f32,
    /// bold weight
    pub bold: bool,
    /// the font family
    pub font: FontRole,
    /// left, right, top and bottom margins
    pub margins: [f32; 4],
    /// what Return does
    pub ret: ReturnKey,
    /// a bullet drawn before the first line
    pub bullet: bool,
    /// a rule drawn above (Section, Program) and below (Program)
    pub rule: bool,
}

impl Style {
    /// Every style, in the order of the style table (the style menu lists the non-generated ones).
    pub const ALL: [Style; 24] = [
        Style::Title,
        Style::Subtitle,
        Style::Subsubtitle,
        Style::Chapter,
        Style::Subchapter,
        Style::Section,
        Style::Subsection,
        Style::Subsubsection,
        Style::Text,
        Style::Code,
        Style::Input,
        Style::Item,
        Style::ItemParagraph,
        Style::Subitem,
        Style::SubitemParagraph,
        Style::Subsubitem,
        Style::ItemNumbered,
        Style::SubitemNumbered,
        Style::SubsubitemNumbered,
        Style::DisplayFormula,
        Style::Program,
        Style::Output,
        Style::Print,
        Style::Message,
    ];

    /// The look and behaviour of the style.
    pub fn info(self) -> StyleInfo {
        use FontRole::*;
        use GroupRule as G;
        use ReturnKey::*;
        let s = |name, key, group, size, font, margins, ret| StyleInfo {
            name,
            key,
            group,
            evaluatable: false,
            generated: false,
            label: false,
            size,
            bold: false,
            font,
            margins,
            ret,
            bullet: false,
            rule: false,
        };
        match self {
            Style::Title => {
                StyleInfo { bold: true, ..s("Title", Some(1), G::Title(0), 45.0, Prose, [27., 10., 30., 10.], NewLine) }
            }
            Style::Subtitle => s("Subtitle", Some(2), G::Title(10), 24.0, Prose, [27., 10., 2., 20.], NewLine),
            Style::Subsubtitle => s("Subsubtitle", None, G::Title(20), 16.0, Prose, [27., 10., 2., 8.], NewLine),
            Style::Chapter => StyleInfo {
                bold: true,
                ..s("Chapter", Some(3), G::Section(10), 34.0, Prose, [27., 10., 15., 10.], NewLine)
            },
            Style::Subchapter => s("Subchapter", None, G::Section(20), 28.0, Prose, [63., 10., 20., 8.], NewLine),
            Style::Section => StyleInfo {
                bold: true,
                rule: true,
                ..s("Section", Some(4), G::Section(30), 28.0, Prose, [27., 10., 18., 8.], NewLine)
            },
            Style::Subsection => StyleInfo {
                bold: true,
                ..s("Subsection", Some(5), G::Section(40), 20.0, Prose, [50., 3., 20., 8.], NewLine)
            },
            Style::Subsubsection => StyleInfo {
                bold: true,
                ..s("Subsubsection", Some(6), G::Section(50), 19.0, Prose, [66., 10., 12., 8.], NewLine)
            },
            Style::Text => s("Text", Some(7), G::Normal, 15.0, Prose, [66., 10., 8., 7.], NewLine),
            Style::Code => StyleInfo {
                evaluatable: true,
                label: true,
                bold: true,
                ..s("Code", Some(8), G::Input, 13.0, Code, [66., 10., 10., 10.], NewLine)
            },
            Style::Input => StyleInfo {
                evaluatable: true,
                label: true,
                bold: true,
                ..s("Input", Some(9), G::Input, 13.0, Code, [66., 10., 10., 5.], NewLine)
            },
            Style::Item => {
                StyleInfo { bullet: true, ..s("Item", None, G::Item(70), 15.0, Prose, [81., 10., 8., 4.], NewCell) }
            }
            Style::ItemParagraph => s("ItemParagraph", None, G::Item(80), 15.0, Prose, [81., 10., 1., 4.], NewCell),
            Style::Subitem => {
                StyleInfo { bullet: true, ..s("Subitem", None, G::Item(80), 14.0, Prose, [105., 12., 4., 4.], NewCell) }
            }
            Style::SubitemParagraph => {
                s("SubitemParagraph", None, G::Item(90), 14.0, Prose, [105., 12., 1., 4.], NewCell)
            }
            Style::Subsubitem => StyleInfo {
                bullet: true,
                ..s("Subsubitem", None, G::Item(90), 13.0, Prose, [129., 12., 4., 4.], NewCell)
            },
            Style::ItemNumbered => s("ItemNumbered", None, G::Item(70), 15.0, Prose, [81., 10., 8., 4.], NewCell),
            Style::SubitemNumbered => {
                s("SubitemNumbered", None, G::Item(80), 14.0, Prose, [105., 12., 4., 4.], NewCell)
            }
            Style::SubsubitemNumbered => {
                s("SubsubitemNumbered", None, G::Item(90), 13.0, Prose, [129., 12., 4., 4.], NewCell)
            }
            Style::DisplayFormula => s("DisplayFormula", None, G::Normal, 14.0, Prose, [66., 10., 6., 6.], NewLine),
            Style::Program => {
                StyleInfo { rule: true, ..s("Program", None, G::Normal, 13.0, Code, [66., 10., 8., 8.], NewLine) }
            }
            Style::Output => StyleInfo {
                generated: true,
                label: true,
                ..s("Output", None, G::Output, 13.0, Code, [66., 10., 5., 10.], Duplicate)
            },
            Style::Print => {
                StyleInfo { generated: true, ..s("Print", None, G::Output, 13.0, Code, [66., 10., 3., 3.], NewLine) }
            }
            Style::Message => {
                StyleInfo { generated: true, ..s("Message", None, G::Output, 11.0, Ui, [66., 10., 3., 3.], ReadOnly) }
            }
        }
    }

    /// The style named `name` (the display name, any case).
    pub fn from_name(name: &str) -> Option<Style> {
        Style::ALL.into_iter().find(|s| s.info().name.eq_ignore_ascii_case(name))
    }

    /// The style of `Alt+digit`.
    pub fn of_key(digit: u8) -> Option<Style> {
        Style::ALL.into_iter().find(|s| s.info().key == Some(digit))
    }

    /// Whether Shift+Enter evaluates cells of this style by default.
    pub fn evaluatable(self) -> bool {
        self.info().evaluatable
    }

    /// Whether cells of this style are made by evaluation.
    pub fn generated(self) -> bool {
        self.info().generated
    }

    /// Whether the content is source code (sent to the kernel, kept straight quotes, no prose quotes).
    pub fn is_code(self) -> bool {
        matches!(self, Style::Input | Style::Code)
    }

    /// The file kind of a non-generated style.
    pub fn kind(self) -> Option<Kind> {
        Some(match self {
            Style::Title => Kind::Title,
            Style::Subtitle => Kind::Subtitle,
            Style::Subsubtitle => Kind::Subsubtitle,
            Style::Chapter => Kind::Chapter,
            Style::Subchapter => Kind::Subchapter,
            Style::Section => Kind::Section,
            Style::Subsection => Kind::Subsection,
            Style::Subsubsection => Kind::Subsubsection,
            Style::Text => Kind::Text,
            Style::Code => Kind::Code,
            Style::Input => Kind::Input,
            Style::Item => Kind::Item,
            Style::ItemParagraph => Kind::ItemParagraph,
            Style::Subitem => Kind::Subitem,
            Style::SubitemParagraph => Kind::SubitemParagraph,
            Style::Subsubitem => Kind::Subsubitem,
            Style::ItemNumbered => Kind::ItemNumbered,
            Style::SubitemNumbered => Kind::SubitemNumbered,
            Style::SubsubitemNumbered => Kind::SubsubitemNumbered,
            Style::DisplayFormula => Kind::DisplayFormula,
            Style::Program => Kind::Program,
            Style::Output | Style::Print | Style::Message => return None,
        })
    }

    /// The style of a file kind.
    pub fn of_kind(kind: Kind) -> Style {
        Style::ALL.into_iter().find(|s| s.kind() == Some(kind)).expect("every kind has a style")
    }

    /// The level of numbered items (1, 2, 3), for their counters.
    pub fn numbering_level(self) -> Option<usize> {
        match self {
            Style::ItemNumbered => Some(1),
            Style::SubitemNumbered => Some(2),
            Style::SubsubitemNumbered => Some(3),
            _ => None,
        }
    }

    /// Styles that reset item numbering (design 2.1: Title, Chapter and Section).
    pub fn resets_numbering(self) -> bool {
        matches!(self, Style::Title | Style::Chapter | Style::Section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_kinds() {
        let keyed: Vec<_> = (1..=9).map(|d| Style::of_key(d).unwrap()).collect();
        use Style::*;
        assert_eq!(keyed, [Title, Subtitle, Chapter, Section, Subsection, Subsubsection, Text, Code, Input]);
        for s in Style::ALL {
            if let Some(k) = s.kind() {
                assert_eq!(Style::of_kind(k), s);
            } else {
                assert!(s.generated());
            }
            assert_eq!(Style::from_name(&s.info().name.to_lowercase()), Some(s));
        }
        assert_eq!(Style::of_key(0), None);
    }
}
