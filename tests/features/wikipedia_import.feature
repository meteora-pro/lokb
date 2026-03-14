Feature: Wikipedia import and search

  Scenario: Add a Wikipedia source and list it
    Given a clean lokb data directory
    And test Wikipedia articles in "wikipedia/"
    When I run lokb "source add wikipedia-test --raw {fixtures}/wikipedia/ --format markdown-dir --class public"
    Then the command succeeds
    And the output contains "added successfully"
    When I run lokb "source list --format json"
    Then the command succeeds
    And the JSON output has a source named "wikipedia-test"

  Scenario: Search Wikipedia articles by keyword
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "search 'Eiffel Tower' --format json"
    Then the command succeeds
    And the JSON search results are not empty
    And the JSON search results contain a document with title "Paris"
    And the JSON search results contain a document from source "wikipedia-test"

  Scenario: Read a specific document
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "read wikipedia-test:Paris"
    Then the command succeeds
    And the output contains "capital and largest city of France"
    And the output contains "## Geography"

  Scenario: Incremental update skips unchanged documents
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "source update wikipedia-test --raw {fixtures}/wikipedia/"
    Then the command succeeds
    And the output contains "Unchanged: 5"
    And the output contains "New:       0"

  Scenario: Duplicate source add is rejected
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "source add wikipedia-test --raw {fixtures}/wikipedia/ --format markdown-dir --class public"
    Then the command fails
    And the output contains "already exists"
