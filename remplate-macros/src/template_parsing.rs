use std::ops::Range;

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

#[derive(Debug)]
struct ParseState {
    rust_string: Option<StringMatchState>,
    last_index: BlockMatchState,
    open_nested_code_blocks: usize,
    code_block_ranges: Vec<Range<usize>>,
    escaped_braces: Vec<usize>,
}

impl std::default::Default for ParseState {
    fn default() -> Self {
        Self {
            rust_string: None,
            last_index: BlockMatchState::Matched(0),
            open_nested_code_blocks: 0,
            code_block_ranges: Vec::new(),
            escaped_braces: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct ParseResult<'a> {
    pub code_block_fragments: Vec<&'a str>,
    pub template_fragments: Vec<&'a str>,
}

#[derive(Debug)]
pub struct MatchError(usize, String);

impl MatchError {
    const TEMPLATE_POINTER_PADDING: usize = 10;

    pub fn abort_with_error(self) -> ! {
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

pub fn parse_template(input: &str) -> Result<ParseResult, MatchError> {
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
                                parse_state
                                    .code_block_ranges
                                    .push(matching_start..last_index);

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
        BlockMatchState::Matched(_) => {
            let mut code_block_fragments = Vec::new();
            let mut template_fragments = Vec::new();
            let mut last_block_end = 0;

            for block in parse_state.code_block_ranges.iter() {
                template_fragments.push(&input[last_block_end..(block.start - 1)]);
                code_block_fragments.push(&input[block.clone()]);
                last_block_end = block.end + 1;
            }

            if let Some(last_template_fragment) = input.get(last_block_end..) {
                template_fragments.push(last_template_fragment);
            }

            Ok(ParseResult {
                code_block_fragments,
                template_fragments,
            })
        }
    }
}
