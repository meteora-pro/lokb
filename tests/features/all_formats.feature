Feature: All data formats end-to-end

  Scenario: EPUB book import and search
    Given a clean lokb data directory
    When I run lokb "source add book --raw {fixtures}/test-book.epub --format epub --class personal"
    Then the command succeeds
    And the output contains "added successfully"
    When I run lokb "search 'ownership borrowing' --format json"
    Then the command succeeds
    And the JSON search results are not empty

  Scenario: GPX track import
    Given a clean lokb data directory
    When I run lokb "source add track --raw {fixtures}/gpx/paris-walk.gpx --format gpx --class personal"
    Then the command succeeds
    And the output contains "added successfully"
    When I run lokb "search 'GPS Track' --format json"
    Then the command succeeds
    And the JSON search results are not empty

  Scenario: CSV data import and search
    Given a clean lokb data directory
    When I run lokb "source add stats --raw {fixtures}/csv/countries.csv --format csv --class public"
    Then the command succeeds
    And the output contains "added successfully"
    When I run lokb "search 'France population' --format json"
    Then the command succeeds
    And the JSON search results are not empty

  Scenario: ZIM file import and search
    Given a clean lokb data directory
    When I run lokb "source add zim-test --raw {fixtures}/test.zim --format zim --class public"
    Then the command succeeds
    And the output contains "added successfully"
    When I run lokb "search 'Rust programming' --format json"
    Then the command succeeds
    And the JSON search results are not empty

  Scenario: HTML directory import
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "search 'quantum computing' --format json"
    Then the command succeeds
    And the JSON search results are not empty

  Scenario: LLM enrich with skip backend (graceful fallback)
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "enrich wikipedia-test --step summarize --llm skip"
    Then the command succeeds
    And the output contains "Enriched 0"

  Scenario: Serve command starts (verify binary accepts it)
    Given a clean lokb data directory
    When I run lokb "serve --help"
    Then the command succeeds
    And the output contains "HTTP API"
