Feature: Privacy and export

  Scenario: Export excludes personal data by default
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    And source "telegram-test" is loaded from "telegram/" as "telegram-export" class "personal"
    When I run lokb "export {tmp}/export.json"
    Then the command succeeds
    And the export file "{tmp}/export.json" contains source "wikipedia-test"
    And the export file "{tmp}/export.json" does not contain source "telegram-test"

  Scenario: Export with --include-personal includes personal data
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    And source "telegram-test" is loaded from "telegram/" as "telegram-export" class "personal"
    When I run lokb "export {tmp}/export-all.json --include-personal"
    Then the command succeeds
    And the export file "{tmp}/export-all.json" contains source "wikipedia-test"
    And the export file "{tmp}/export-all.json" contains source "telegram-test"
