Feature: git-ast canonical clean/smudge round-trip
  These scenarios are the README's behavioural claims, made executable against
  real git with the built binary installed as the clean/smudge filter.

  Background:
    Given a repository with git-ast installed

  Scenario: Reformatting never reaches history
    When I stage "calc.rs" containing:
      """
      fn add(a:i32,b:i32)->i32{a+b}
      """
    And I commit
    And I overwrite "calc.rs" with:
      """
      fn add( a : i32 , b : i32 ) -> i32 {

          a + b
      }
      """
    Then "calc.rs" shows no diff

  Scenario: A real change still shows a diff
    When I stage "calc.rs" containing:
      """
      fn add(a:i32,b:i32)->i32{a+b}
      """
    And I commit
    And I overwrite "calc.rs" with:
      """
      fn add(a: i32, b: i32) -> i32 {
          a - b
      }
      """
    Then "calc.rs" shows a diff

  Scenario: Different formattings store byte-identical blobs
    When I stage "a.rs" containing:
      """
      fn add(a:i32,b:i32)->i32{a+b}
      """
    And I stage "b.rs" containing:
      """
      fn add( a : i32 , b : i32 ) -> i32 {
          a + b
      }
      """
    Then the stored blobs for "a.rs" and "b.rs" are identical

  Scenario: Round-trip restores canonical source on checkout
    When I stage "f.rs" containing:
      """
      fn f()->i32{1+2}
      """
    And I commit
    And I check out "f.rs" fresh
    Then the working file "f.rs" is:
      """
      fn f() -> i32 {
          1 + 2
      }
      """

  Scenario: Syntax errors are rejected (fail-closed)
    Then staging "bad.rs" containing "fn main( {" is rejected

  Scenario: Non-Rust files pass through unchanged
    When I stage "notes.txt" containing "  spaced  text  "
    Then the stored blob for "notes.txt" is "  spaced  text  "

  Scenario: JSON reformatting never reaches history
    When I stage "config.json" containing:
      """
      { "b": 1, "a": 2 }
      """
    And I commit
    And I overwrite "config.json" with:
      """
      {
          "a":  2,
          "b":  1
      }
      """
    Then "config.json" shows no diff

  Scenario: Different JSON formattings store byte-identical blobs
    When I stage "x.json" containing:
      """
      {"a":1,"b":2}
      """
    And I stage "y.json" containing:
      """
      { "b": 2,
        "a": 1 }
      """
    Then the stored blobs for "x.json" and "y.json" are identical

  Scenario: JSON round-trip restores canonical source on checkout
    When I stage "config.json" containing:
      """
      { "b": 1, "a": 2 }
      """
    And I commit
    And I check out "config.json" fresh
    Then the working file "config.json" is:
      """
      {
        "a": 2,
        "b": 1
      }
      """

  Scenario: Invalid JSON is rejected (fail-closed)
    Then staging "bad.json" containing "{ not valid }" is rejected

  Scenario: Structural merge — edits to different keys merge cleanly
    When I stage "config.json" containing:
      """
      { "a": 1, "b": 1 }
      """
    And I commit
    And I branch "theirs"
    And I stage "config.json" containing:
      """
      { "a": 1, "b": 3 }
      """
    And I commit
    And I check out the original branch
    And I stage "config.json" containing:
      """
      { "a": 2, "b": 1 }
      """
    And I commit
    And I merge "theirs"
    Then the merge succeeds
    And the working file "config.json" is:
      """
      {
        "a": 2,
        "b": 3
      }
      """

  Scenario: Structural merge — same key diverging is a conflict
    When I stage "config.json" containing:
      """
      { "a": 1 }
      """
    And I commit
    And I branch "theirs"
    And I stage "config.json" containing:
      """
      { "a": 3 }
      """
    And I commit
    And I check out the original branch
    And I stage "config.json" containing:
      """
      { "a": 2 }
      """
    And I commit
    And I merge "theirs"
    Then the merge conflicts

  Scenario: Structural merge — nested objects, different sub-keys merge
    When I stage "config.json" containing:
      """
      { "o": { "x": 1, "y": 1 } }
      """
    And I commit
    And I branch "theirs"
    And I stage "config.json" containing:
      """
      { "o": { "x": 1, "y": 3 } }
      """
    And I commit
    And I check out the original branch
    And I stage "config.json" containing:
      """
      { "o": { "x": 2, "y": 1 } }
      """
    And I commit
    And I merge "theirs"
    Then the merge succeeds
    And the working file "config.json" is:
      """
      {
        "o": {
          "x": 2,
          "y": 3
        }
      }
      """
