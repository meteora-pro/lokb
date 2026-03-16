Feature: File watcher and incremental sync

  Scenario: Source add with --watch flag accepted
    Given a clean lokb data directory
    When I run lokb "source add wiki --raw {fixtures}/wikipedia/ --format markdown-dir --class public --help"
    Then the command succeeds
    And the output contains "watch"

  Scenario: Incremental update detects new files
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "source update wikipedia-test --raw {fixtures}/wikipedia/"
    Then the command succeeds
    And the output contains "Unchanged: 5"
