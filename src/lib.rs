use std::{
    collections::VecDeque,
    iter,
    path::{Path, PathBuf},
};

#[derive(Debug, PartialEq, Eq)]
enum BlockMatchState {
    Matching { start: usize, current: usize },
    Matched(usize),
}

#[derive(Debug)]
struct StringMatch {
    position: usize,
    length: usize,
}

#[derive(Debug)]
enum StringMatchState {
    MatchingFirst(StringMatch),
    MatchingSecond {
        first: StringMatch,
        second: Option<StringMatch>,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct CodeBlock {
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct ParseState {
    rust_string: Option<StringMatchState>,
    last_index: BlockMatchState,
    open_nested_code_blocks: usize,
    code_blocks: Vec<CodeBlock>,
    escaped_braces: Vec<usize>,
}

impl std::default::Default for ParseState {
    fn default() -> Self {
        Self {
            rust_string: None,
            last_index: BlockMatchState::Matched(0),
            open_nested_code_blocks: 0,
            code_blocks: Vec::new(),
            escaped_braces: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct ParseResult {
    code_blocks: Vec<CodeBlock>,
}

#[derive(Debug)]
struct MatchError(usize, String);

impl MatchError {
    const TEMPLATE_POINTER_PADDING: usize = 10;

    fn abort_with_error(self) -> ! {
        let left_slice = self.left_erroneous_slice();
        let right_slice = self.right_erroneous_slice();
        let erroneous_character = self.erroneous_character();

        let MatchError(erroneous_character_position, _) = self;

        let allowed_erroneous_slice_length =
            std::cmp::min(left_slice.len() - 1, right_slice.len() - 1);

        let final_left_slice = &left_slice
            [(left_slice.len() - allowed_erroneous_slice_length)..(left_slice.len() - 1)];

        let final_right_slice = &right_slice[1..allowed_erroneous_slice_length];

        let erroneous_slice = [final_left_slice, &erroneous_character, final_right_slice].concat();

        let mut pointer: String = (0..allowed_erroneous_slice_length).map(|_| " ").collect();
        pointer.push('^');

        // pointer.replace_range(pointer_pos..(pointer_pos + 1), "^");

        panic!(
            "Failed to find closing token for `{}` at position {}:\n\"{}\"\n{}",
            erroneous_character, erroneous_character_position, erroneous_slice, pointer
        )
    }

    fn left_erroneous_slice(&self) -> String {
        let MatchError(erroneous_character_position, template) = self;
        let slice_start = erroneous_character_position
            .checked_sub(Self::TEMPLATE_POINTER_PADDING)
            .unwrap_or(0);
        format!(
            "{:?}",
            &template[slice_start..*erroneous_character_position]
        )
    }

    fn right_erroneous_slice(&self) -> String {
        let MatchError(erroneous_character_position, template) = self;
        let slice_end = std::cmp::min(
            erroneous_character_position + Self::TEMPLATE_POINTER_PADDING,
            template.len(),
        );
        format!(
            "{:?}",
            &template[(*erroneous_character_position + 1)..slice_end]
        )
    }

    fn erroneous_character(&self) -> String {
        let MatchError(erroneous_character_position, template) = self;
        template[*erroneous_character_position..(*erroneous_character_position + 1)].to_string()
    }
}

fn parse_code_blocks(input: &str) -> Result<ParseResult, MatchError> {
    let mut parse_state = ParseState {
        last_index: BlockMatchState::Matched(0),
        ..Default::default()
    };

    while let BlockMatchState::Matched(last_index) = parse_state.last_index {
        match input.get(last_index..).and_then(|substr| substr.find('{')) {
            Some(index) => {
                parse_state.last_index = BlockMatchState::Matching {
                    start: last_index + index + 1,
                    current: last_index + index + 1,
                };

                while let BlockMatchState::Matching {
                    start: matching_start,
                    current: last_index,
                } = parse_state.last_index
                {
                    let substr = input.get(last_index..last_index + 1);
                    match substr {
                        Some("{") => {
                            if last_index == matching_start {
                                parse_state.escaped_braces.push(last_index - 1);
                                parse_state.last_index = BlockMatchState::Matched(last_index + 1);
                            } else {
                                match parse_state.rust_string {
                                    None => {
                                        parse_state.open_nested_code_blocks += 1;
                                    }
                                    Some(_) => {}
                                }
                            }
                        }
                        Some("r") => match parse_state.rust_string {
                            None | Some(StringMatchState::MatchingFirst(_)) => {
                                parse_state.rust_string =
                                    Some(StringMatchState::MatchingFirst(StringMatch {
                                        position: last_index,
                                        length: 0,
                                    }));
                            }
                            Some(StringMatchState::MatchingSecond { first, .. }) => {
                                parse_state.rust_string = Some(StringMatchState::MatchingSecond {
                                    first,
                                    second: Some(StringMatch {
                                        position: last_index,
                                        length: 0,
                                    }),
                                })
                            }
                        },
                        Some("#") => match parse_state.rust_string {
                            Some(StringMatchState::MatchingFirst(first)) => {
                                parse_state.rust_string =
                                    Some(StringMatchState::MatchingFirst(StringMatch {
                                        position: first.position,
                                        length: first.length + 1,
                                    }))
                            }
                            Some(StringMatchState::MatchingSecond {
                                first:
                                    StringMatch {
                                        position: _,
                                        length: first_length,
                                    },
                                second:
                                    Some(StringMatch {
                                        position: _,
                                        length: second_length,
                                    }),
                            }) if first_length == (second_length + 1) => {
                                parse_state.rust_string = None
                            }
                            Some(StringMatchState::MatchingSecond {
                                first,
                                second: Some(second),
                            }) => {
                                parse_state.rust_string = Some(StringMatchState::MatchingSecond {
                                    first,
                                    second: Some(StringMatch {
                                        position: second.position,
                                        length: second.length + 1,
                                    }),
                                })
                            }
                            None | Some(_) => {}
                        },
                        Some("\"") => match parse_state.rust_string {
                            Some(StringMatchState::MatchingFirst(first)) => {
                                parse_state.rust_string = Some(StringMatchState::MatchingSecond {
                                    first: StringMatch {
                                        position: first.position,
                                        length: first.length,
                                    },
                                    second: None,
                                })
                            }
                            Some(StringMatchState::MatchingSecond {
                                first:
                                    StringMatch {
                                        position: _,
                                        length: 0,
                                    },
                                second: None,
                            }) => parse_state.rust_string = None,
                            Some(StringMatchState::MatchingSecond {
                                first,
                                second: None | Some(_),
                            }) => {
                                parse_state.rust_string = Some(StringMatchState::MatchingSecond {
                                    first,
                                    second: Some(StringMatch {
                                        position: last_index,
                                        length: 0,
                                    }),
                                })
                            }
                            None => {
                                parse_state.rust_string = Some(StringMatchState::MatchingSecond {
                                    first: StringMatch {
                                        position: last_index,
                                        length: 0,
                                    },
                                    second: None,
                                })
                            }
                        },
                        Some("}") if parse_state.rust_string.is_none() => {
                            if parse_state.open_nested_code_blocks > 0 {
                                parse_state.open_nested_code_blocks -= 1;
                            } else {
                                parse_state.last_index = BlockMatchState::Matched(last_index + 1);
                                parse_state.code_blocks.push(CodeBlock {
                                    start: matching_start,
                                    end: last_index,
                                });

                                break;
                            }
                        }
                        None => break,
                        _ => match parse_state.rust_string {
                            Some(StringMatchState::MatchingFirst(_)) => {
                                parse_state.rust_string = None
                            }
                            Some(StringMatchState::MatchingSecond {
                                first,
                                second: Some(_),
                            }) => {
                                parse_state.rust_string = Some(StringMatchState::MatchingSecond {
                                    first,
                                    second: None,
                                })
                            }
                            None | Some(_) => {}
                        },
                    }

                    match parse_state.last_index {
                        BlockMatchState::Matching { start, .. } => {
                            parse_state.last_index = BlockMatchState::Matching {
                                start,
                                current: last_index + 1,
                            }
                        }
                        BlockMatchState::Matched(_) => {}
                    };
                }
            }
            None => {
                break;
            }
        }
    }

    match parse_state.last_index {
        BlockMatchState::Matching { start, .. } => Err(MatchError(start - 1, input.to_string())),
        BlockMatchState::Matched(_) => Ok(ParseResult {
            code_blocks: parse_state.code_blocks,
        }),
    }
}

#[allow(dead_code)]
#[derive(PartialEq, Eq, Debug)]
enum Item {
    EscapedBrace(usize),
    CodeBlock(CodeBlock),
}

impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Item {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Item::EscapedBrace(pos1), Item::EscapedBrace(pos2)) => pos1.cmp(pos2),
            (Item::EscapedBrace(pos1), Item::CodeBlock(CodeBlock { start: pos2, .. })) => {
                pos1.cmp(pos2)
            }
            (Item::CodeBlock(CodeBlock { start: pos1, .. }), Item::EscapedBrace(pos2)) => {
                pos1.cmp(pos2)
            }
            (
                Item::CodeBlock(CodeBlock { start: pos1, .. }),
                Item::CodeBlock(CodeBlock { start: pos2, .. }),
            ) => pos1.cmp(pos2),
        }
    }
}

struct FormatPart<'a> {
    expression: &'a str,
    formatting: Option<&'a str>,
}

impl<'a> From<&'a str> for FormatPart<'a> {
    fn from(value: &'a str) -> Self {
        if let Some(position) = value.find(':') {
            let (expression, formatting) = value.split_at(position);
            FormatPart {
                expression,
                formatting: Some(formatting),
            }
        } else {
            FormatPart {
                expression: value,
                formatting: None,
            }
        }
    }
}

impl<'a> FormatPart<'a> {
    fn to_code(&self) -> String {
        match self {
            FormatPart {
                expression,
                formatting: Some(format_part),
            } => format!(
                r#"result.push_str(&format!("{{{}}}", {}));"#,
                format_part, expression
            ),
            FormatPart {
                expression,
                formatting: None,
            } => format!(r#"result.push_str(&format!("{{{}}}"));"#, expression),
        }
    }
}

fn obtain_format_part(code_block: &str) -> (Option<&str>, Option<FormatPart>) {
    match code_block.rfind(';') {
        Some(position) => {
            let format_part = match code_block[(position + 1)..].trim() {
                "" => None,
                other => Some(other),
            };

            (
                Some(&code_block[..position + 1]),
                format_part.map(FormatPart::from),
            )
        }
        None => {
            let format_part = match code_block.trim() {
                "" => None,
                other => Some(other),
            };

            (None, format_part.map(FormatPart::from))
        }
    }
}

fn handle_input(input: &str) -> Result<String, MatchError> {
    let parse_result = parse_code_blocks(input)?;

    let mut code_blocks = VecDeque::new();
    let mut template_fragments = VecDeque::new();
    let mut last_block_end = 0;

    for block in parse_result.code_blocks.iter() {
        template_fragments.push_back(input[last_block_end..(block.start - 1)].to_string());
        code_blocks.push_back(input[block.start..block.end].to_string());
        last_block_end = block.end + 1;
    }

    if let Some(last_template_fragment) = input.get(last_block_end..) {
        template_fragments.push_back(last_template_fragment.to_string());
    }

    let start = format!(
        r#"let mut result = String::from("{}");"#,
        &template_fragments.pop_front().unwrap()
    );
    let end = "result";
    let mut code = String::new();

    code.push_str(&start);

    if let Some(code_block) = &code_blocks.pop_front() {
        match obtain_format_part(code_block) {
            (None, None) => unreachable!(),
            (None, Some(format_part)) => {
                code.push_str(&format_part.to_code());
            }
            (Some(code_block), None) => {
                code.push_str(code_block);
            }
            (Some(code_block), Some(format_part)) => {
                code.push_str(code_block);
                code.push_str(&format_part.to_code());
            }
        }
    }

    for (template, code_block) in iter::zip(&template_fragments, &code_blocks) {
        code.push_str(&format!(r#"result.push_str("{}");"#, template));

        match obtain_format_part(code_block) {
            (None, None) => unreachable!(),
            (None, Some(format_part)) => {
                code.push_str(&format_part.to_code());
            }
            (Some(code_block), None) => {
                code.push_str(code_block);
            }
            (Some(code_block), Some(format_part)) => {
                code.push_str(code_block);
                code.push_str(&format_part.to_code());
            }
        }
    }

    if let Some(template_part) = template_fragments.pop_back() {
        code.push_str(&format!(r#"result.push_str("{}");"#, template_part));
    }
    //

    code.push_str(end);

    Ok(code)
}

fn create_include_bytes(file_path: &PathBuf) -> String {
    format!(r#"include_bytes!({:?});"#, file_path)
}

#[derive(Debug)]
struct PathResolutionError {
    path: PathBuf,
    source: std::io::Error,
}

impl std::fmt::Display for PathResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} - {:?}", self.source, self.path))
    }
}

impl From<(PathBuf, std::io::Error)> for PathResolutionError {
    fn from(value: (PathBuf, std::io::Error)) -> Self {
        Self {
            path: value.0,
            source: value.1,
        }
    }
}

fn canonicalize_path<P>(path: P) -> Result<PathBuf, PathResolutionError>
where
    P: AsRef<Path>,
{
    let mut canonicalized_path = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR environment variable"),
    );
    canonicalized_path.push(path.as_ref());

    std::fs::canonicalize(canonicalized_path.clone()).map_err(|e| (canonicalized_path, e).into())
}

#[proc_macro]
pub fn remplate(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input_str = input.to_string();
    let template_path = input_str.trim_matches('"');

    let canonicalized_path = match canonicalize_path(template_path) {
        Ok(path) => path,
        Err(error) => panic!("{}", error),
    };

    let file_content = match std::fs::read_to_string(&canonicalized_path) {
        Ok(content) => content,
        Err(error) => panic!("{:?}", error),
    };

    format!(
        r"{{
            {}
            {}
        }}",
        create_include_bytes(&canonicalized_path),
        match handle_input(&file_content) {
            Ok(definition) => definition,
            Err(error) => error.abort_with_error(),
        },
    )
    .parse()
    .unwrap()
}
