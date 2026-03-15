Feature: Semantic search with embeddings

  @slow
  Scenario: Semantic search finds documents without exact keyword match
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "search 'tall metal structure in France' --format json"
    Then the command succeeds
    And the JSON search results are not empty

  @slow
  Scenario: Hybrid search combines FTS and vector results
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "search 'quantum physics experiments' --format json"
    Then the command succeeds
    And the JSON search results are not empty
    And the JSON search results contain a document with title "Quantum computing"

  Scenario: Search works without embeddings (FTS fallback)
    Given a clean lokb data directory
    And source "wikipedia-test" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "search 'Eiffel Tower' --format json"
    Then the command succeeds
    And the JSON search results are not empty
