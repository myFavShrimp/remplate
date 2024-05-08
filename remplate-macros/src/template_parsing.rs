use std::{ops::Range, path::PathBuf};

#[derive(PartialEq, Eq, Debug, Default)]
pub struct ParseResult {
    pub code_block_fragment_ranges: Vec<Range<usize>>,
    pub template_fragment_ranges: Vec<Range<usize>>,
}

#[derive(PartialEq, Eq, Debug)]
pub enum TemplateParseError {
    CodeBlockHasNoEnd { position: usize },
    StrHasNoEnd { position: usize },
}

impl<'a> TemplateParseError {
    pub fn into(
        self,
        template_path: &'a PathBuf,
        template: &'a str,
        error_span: proc_macro2::Span,
    ) -> crate::error::TemplateError<'a> {
        let position = match self {
            TemplateParseError::CodeBlockHasNoEnd { position } => position,
            TemplateParseError::StrHasNoEnd { position } => position,
        };

        crate::error::TemplateError(
            position..(position + 1),
            template_path,
            template,
            crate::error::TemplateErrorKind::ClosingToken,
            error_span,
        )
    }
}

pub fn parse_template(input: &str) -> Result<ParseResult, TemplateParseError> {
    let mut result = ParseResult::default();
    let mut iterator = input.chars().enumerate();

    while let Some((index, character)) = iterator.next() {
        match character {
            '{' => match parse_code_block(&input[index..]) {
                Ok(block_end) => {
                    match result.code_block_fragment_ranges.last() {
                        Some(last_block) => {
                            result
                                .template_fragment_ranges
                                .push((last_block.end + 1)..index);
                        }
                        None => {
                            result.template_fragment_ranges.push(0..index);
                        }
                    }

                    result
                        .code_block_fragment_ranges
                        .push((index + 1)..(block_end + index));

                    iterator.nth(block_end - 1);
                }
                Err(error) => match error {
                    CodeBlockParseError::StrHasNoEnd { start } => {
                        return Err(TemplateParseError::StrHasNoEnd {
                            position: start + index,
                        })
                    }
                    CodeBlockParseError::BlockHasNoEnd => {
                        return Err(TemplateParseError::CodeBlockHasNoEnd { position: index })
                    }
                    CodeBlockParseError::Escaped => continue,
                },
            },
            _ => {}
        }
    }

    let last_block = result.code_block_fragment_ranges.last().unwrap();
    result
        .template_fragment_ranges
        .push((last_block.end + 1)..input.len());

    Ok(result)
}

#[derive(PartialEq, Eq, Debug)]
pub enum CodeBlockParseError {
    StrHasNoEnd { start: usize },
    BlockHasNoEnd,
    Escaped,
}

fn parse_code_block(input: &str) -> Result<usize, CodeBlockParseError> {
    let mut iterator = input.chars().enumerate();
    let mut open_delimiters = 0;

    while let Some((index, character)) = iterator.next() {
        match character {
            '{' => {
                if index == 1 {
                    return Err(CodeBlockParseError::Escaped);
                } else if index > 0 {
                    open_delimiters += 1;
                }
            }
            'r' | '"' => match parse_str_literal(&input[index..]) {
                Ok(str_range) => {
                    iterator.nth(str_range.end - 1);
                }
                Err(StrLiteralParseError::NoStrFound) => continue,
                Err(StrLiteralParseError::StrHasNoEnd { start }) => {
                    return Err(CodeBlockParseError::StrHasNoEnd {
                        start: start + index,
                    })
                }
            },
            '}' => {
                if open_delimiters == 0 {
                    return Ok(index);
                } else {
                    open_delimiters -= 1;
                }
            }
            _ => {}
        }
    }

    Err(CodeBlockParseError::BlockHasNoEnd)
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

#[derive(PartialEq, Eq, Debug)]
pub enum StrLiteralParseError {
    NoStrFound,
    StrHasNoEnd { start: usize },
}

fn parse_str_literal(input: &str) -> Result<Range<usize>, StrLiteralParseError> {
    let mut parse_state = None;

    for (index, character) in input.chars().enumerate() {
        match character {
            'r' => match parse_state {
                None | Some(StringMatchState::MatchingFirst(_)) => {
                    parse_state = Some(StringMatchState::MatchingFirst(StringMatch {
                        position: index,
                        length: 0,
                    }));
                }
                Some(StringMatchState::MatchingSecond { .. }) => continue,
            },
            '#' => match parse_state {
                Some(StringMatchState::MatchingFirst(first)) => {
                    parse_state = Some(StringMatchState::MatchingFirst(StringMatch {
                        position: first.position,
                        length: first.length + 1,
                    }))
                }
                Some(StringMatchState::MatchingSecond {
                    first: first_match,
                    second: Some(second_match),
                }) if first_match.length == (second_match.length + 1) => {
                    parse_state = Some(StringMatchState::MatchingSecond {
                        first: first_match,
                        second: Some(StringMatch {
                            position: second_match.position,
                            length: second_match.length + 1,
                        }),
                    });
                    break;
                }
                Some(StringMatchState::MatchingSecond {
                    first,
                    second: Some(second),
                }) => {
                    parse_state = Some(StringMatchState::MatchingSecond {
                        first,
                        second: Some(StringMatch {
                            position: second.position,
                            length: second.length + 1,
                        }),
                    })
                }
                None | Some(_) => {}
            },
            '"' => {
                match parse_state {
                    Some(StringMatchState::MatchingFirst(first)) => {
                        parse_state = Some(StringMatchState::MatchingSecond {
                            first: StringMatch {
                                position: first.position,
                                length: first.length,
                            },
                            second: None,
                        })
                    }
                    Some(StringMatchState::MatchingSecond {
                        first,
                        second: None,
                    }) if first.length == 0 => {
                        parse_state = Some(StringMatchState::MatchingSecond {
                            first,
                            second: Some(StringMatch {
                                position: index,
                                length: 0,
                            }),
                        });

                        break;
                    }
                    Some(StringMatchState::MatchingSecond {
                        first,
                        second: None | Some(_),
                    }) => {
                        parse_state = Some(StringMatchState::MatchingSecond {
                            first,
                            second: Some(StringMatch {
                                position: index,
                                length: 0,
                            }),
                        });
                    }
                    None => {
                        parse_state = Some(StringMatchState::MatchingSecond {
                            first: StringMatch {
                                position: index,
                                length: 0,
                            },
                            second: None,
                        })
                    }
                };
            }
            _ => match parse_state {
                Some(StringMatchState::MatchingFirst(_)) => break,
                Some(StringMatchState::MatchingSecond {
                    first,
                    second: Some(_),
                }) => {
                    parse_state = Some(StringMatchState::MatchingSecond {
                        first,
                        second: None,
                    })
                }
                None => {
                    break;
                }
                Some(_) => {}
            },
        }
    }

    match parse_state {
        Some(parse_state) => match parse_state {
            StringMatchState::MatchingFirst(_) => Err(StrLiteralParseError::NoStrFound),
            StringMatchState::MatchingSecond {
                first,
                second: None,
            } => Err(StrLiteralParseError::StrHasNoEnd {
                start: first.position,
            }),
            StringMatchState::MatchingSecond {
                first,
                second: Some(second),
            } if second.length != first.length => Err(StrLiteralParseError::StrHasNoEnd {
                start: first.position,
            }),
            StringMatchState::MatchingSecond {
                first,
                second: Some(second),
            } => Ok(first.position..(second.position + second.length)),
        },
        None => Err(StrLiteralParseError::NoStrFound),
    }
}

#[cfg(test)]
mod template_parse_tests {
    use crate::template_parsing::TemplateParseError;

    use super::{parse_template, ParseResult};

    #[test]
    fn parse_html_template() {
        let to_parse = "<h1>{let x = 15;}{x}</h1>";
        let result = parse_template(to_parse);
        assert_eq!(
            result,
            Ok(ParseResult {
                code_block_fragment_ranges: vec![5..16, 18..19],
                template_fragment_ranges: vec![0..4, 17..17, 20..25],
            })
        )
    }

    #[test]
    fn parse_broken_html_template_unclosed_delimiter() {
        let to_parse = "<h1>{let x = {15;}{x}</h1>";
        let result = parse_template(to_parse);
        assert_eq!(
            result,
            Err(TemplateParseError::CodeBlockHasNoEnd { position: 4 })
        )
    }

    #[test]
    fn parse_broken_html_template_unclosed_delimiter_2() {
        let to_parse = r#"<h1>{let x = "15;}{x}</h1>"#;
        let result = parse_template(to_parse);
        assert_eq!(
            result,
            Err(TemplateParseError::StrHasNoEnd { position: 13 })
        )
    }
}

#[cfg(test)]
mod code_block_parse_tests {
    use super::{parse_code_block, CodeBlockParseError};

    #[test]
    fn parse_block() {
        let to_parse = "{let x = 15;} <br/>";
        let result = parse_code_block(to_parse);
        assert_eq!(result, Ok(12))
    }

    #[test]
    fn parse_block_without_end() {
        let to_parse = "{let x = 15; <br/>";
        let result = parse_code_block(to_parse);
        assert_eq!(result, Err(CodeBlockParseError::BlockHasNoEnd))
    }

    #[test]
    fn parse_escaped_block() {
        let to_parse = "{{ <br/>";
        let result = parse_code_block(to_parse);
        assert_eq!(result, Err(CodeBlockParseError::Escaped))
    }

    #[test]
    fn parse_block_with_str_literal() {
        let to_parse = r#"{let x = "my str";} <br/>"#;
        let result = parse_code_block(to_parse);
        assert_eq!(result, Ok(18))
    }

    #[test]
    fn parse_block_with_r_str_literal() {
        let to_parse = r##"{let x = r#"my "str"#;} <br/>"##;
        let result = parse_code_block(to_parse);
        assert_eq!(result, Ok(22))
    }

    #[test]
    fn parse_block_with_multiple_str_literal() {
        let to_parse = r##"{let x = r#"my "str"#; let y = "second str"; } <br/>"##;
        let result = parse_code_block(to_parse);
        assert_eq!(result, Ok(45))
    }

    #[test]
    fn parse_block_with_format_expression() {
        let to_parse = r##"{let x = r#"my "str"#; x:? } <br/>"##;
        let result = parse_code_block(to_parse);
        assert_eq!(result, Ok(27))
    }

    #[test]
    fn parse_block_with_two_str() {
        let to_parse = r##"{"1""2"}"##;
        let result = parse_code_block(to_parse);
        assert_eq!(result, Ok(7))
    }
}

#[cfg(test)]
mod str_parse_tests {
    use super::{parse_str_literal, StrLiteralParseError};

    #[test]
    fn parse_str_lit() {
        let to_parse = r###""some " text" rest"###;
        let result = parse_str_literal(to_parse);
        assert_eq!(result, Ok(0..6))
    }

    #[test]
    fn parse_r_str_lit() {
        let to_parse = r###"r##"some"# "## text"## rest"###;
        let result = parse_str_literal(to_parse);
        assert_eq!(result, Ok(0..13))
    }

    #[test]
    fn parse_no_str_lit_at_start() {
        let to_parse = r###"start "some " text" rest"###;
        let result = parse_str_literal(to_parse);
        assert_eq!(result, Err(StrLiteralParseError::NoStrFound))
    }

    #[test]
    fn parse_no_r_str_lit_at_start() {
        let to_parse = r###"start r##"some"# "## text"##"###;
        let result = parse_str_literal(to_parse);
        assert_eq!(result, Err(StrLiteralParseError::NoStrFound))
    }

    #[test]
    fn parse_no_r_str_lit_end() {
        let to_parse = r###"r##"some"# text "###;
        let result = parse_str_literal(to_parse);
        assert_eq!(result, Err(StrLiteralParseError::StrHasNoEnd { start: 0 }))
    }

    #[test]
    fn parse_no_str_lit_end() {
        let to_parse = r###""some text "###;
        let result = parse_str_literal(to_parse);
        assert_eq!(result, Err(StrLiteralParseError::StrHasNoEnd { start: 0 }))
    }
}
