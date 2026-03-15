Feature: Personal data formats and privacy

  Scenario: Import MBOX email and search
    Given a clean lokb data directory
    When I run lokb "source add email --raw {fixtures}/email/test.mbox --format mbox --class personal"
    Then the command succeeds
    And the output contains "added successfully"
    When I run lokb "search 'project proposal' --format json"
    Then the command succeeds
    And the JSON search results are not empty

  Scenario: Contact entities extracted from email
    Given a clean lokb data directory
    When I run lokb "source add email --raw {fixtures}/email/test.mbox --format mbox --class personal"
    Then the command succeeds
    And the output contains "Contacts"

  Scenario: Contact entities extracted from Telegram
    Given a clean lokb data directory
    And source "telegram-test" is loaded from "telegram/" as "telegram-export" class "personal"
    When I run lokb "storage status --format json"
    Then the command succeeds
    And the output contains "entity_count"

  Scenario: Personal email excluded from public export
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "source add email --raw {fixtures}/email/test.mbox --format mbox --class personal"
    Then the command succeeds
    When I run lokb "export {tmp}/export.json"
    Then the command succeeds
    And the export file "{tmp}/export.json" contains source "wikipedia-test"
    And the export file "{tmp}/export.json" does not contain source "email"
