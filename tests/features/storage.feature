Feature: Storage status

  Scenario: Storage status shows layer sizes after ingestion
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "storage status --format json"
    Then the command succeeds
    And the JSON storage "source" layer size is greater than 0
