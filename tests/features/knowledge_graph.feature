Feature: Knowledge graph — entities, relations, lookup

  Scenario: Entity extraction from wikilinks
    Given a clean lokb data directory
    And source "wiki" is loaded from "wiki-with-links/" as "markdown-dir" class "public"
    When I run lokb "entity Paris --format json"
    Then the command succeeds
    And the output contains "Paris"
    And the output contains "mention_count"

  Scenario: Entity co-occurrence relations
    Given a clean lokb data directory
    And source "wiki" is loaded from "wiki-with-links/" as "markdown-dir" class "public"
    When I run lokb "entity France --relations --format json"
    Then the command succeeds
    And the output contains "co_occurs_with"
    And the output contains "relations"

  Scenario: Entity search with fuzzy fallback
    Given a clean lokb data directory
    And source "wiki" is loaded from "wiki-with-links/" as "markdown-dir" class "public"
    When I run lokb "entity Nonexistent"
    Then the command succeeds
    And the output contains "not found"

  Scenario: Storage status shows knowledge graph stats
    Given a clean lokb data directory
    And source "wiki" is loaded from "wiki-with-links/" as "markdown-dir" class "public"
    When I run lokb "storage status --format json"
    Then the command succeeds
    And the output contains "entity_count"
    And the output contains "relation_count"

  Scenario: Fact lookup with fallback to search
    Given a clean lokb data directory
    And source "wiki" is loaded from "wiki-with-links/" as "markdown-dir" class "public"
    When I run lokb "lookup 'capital of France'"
    Then the command succeeds
