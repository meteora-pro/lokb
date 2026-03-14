Feature: Personal data import and search

  Scenario: Import Telegram chat and search messages
    Given a clean lokb data directory
    And source "telegram-test" is loaded from "telegram/" as "telegram-export" class "personal"
    When I run lokb "search 'restaurant' --format json"
    Then the command succeeds
    And the JSON search results are not empty
    And the JSON search results contain a document from source "telegram-test"

  Scenario: Personal-only filter returns only personal sources
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    And source "telegram-test" is loaded from "telegram/" as "telegram-export" class "personal"
    When I run lokb "search 'Paris' --personal-only --format json"
    Then the command succeeds
    And the JSON search results only contain documents from source "telegram-test"

  Scenario: Public-only filter excludes personal sources
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    And source "telegram-test" is loaded from "telegram/" as "telegram-export" class "personal"
    When I run lokb "search 'quantum' --public-only --format json"
    Then the command succeeds
    And the JSON search results are not empty
    And the JSON search results only contain documents from source "wikipedia-test"
