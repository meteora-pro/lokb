# Примеры тестовых данных для интеграционных тестов

Этот документ содержит примеры реальных данных для test fixtures.

## Wikipedia Mini - Пример статьи

### tests/fixtures/wikipedia-mini/Geography/Paris.md

```markdown
---
title: "Paris"
external_id: "wiki:Paris"
language: "en"
category: "Geography"
created_at: "2024-01-01T00:00:00Z"
---

# Paris

Paris is the capital and most populous city of France. With an official estimated population of 2,102,650 residents as of 1 January 2023 in an area of more than 105 km², Paris is the fourth-largest city in the European Union and the 30th most densely populated city in the world in 2022.

## Geography

Paris is located in northern France, on the banks of the River Seine. The city proper is small; no more than about 10 km (6 miles) from north to south and east to west.

## History

The city of Paris was founded in the 3rd century BC by a Celtic people called the Parisii. The Romans conquered the Paris Basin in 52 BC and built their own version of the city on the Left Bank.

## Culture

Paris is known for its museums and architectural landmarks. The Louvre is the world's most visited art museum. Other famous landmarks include the Eiffel Tower, Notre-Dame Cathedral, and the Arc de Triomphe.

## Economy

Paris has the sixth-largest GDP in the world and the largest in Europe. The city is a major hub for finance, commerce, fashion, science, and arts.
```

### tests/fixtures/wikipedia-mini/Science/Quantum_Computing.md

```markdown
---
title: "Quantum Computing"
external_id: "wiki:Quantum_Computing"
language: "en"
category: "Science"
created_at: "2024-01-01T00:00:00Z"
---

# Quantum Computing

Quantum computing is a type of computation that harnesses the collective properties of quantum states, such as superposition, interference, and entanglement, to perform calculations.

## Principles

A quantum computer uses qubits, which can exist in superposition—meaning they can be in both 0 and 1 states simultaneously. This allows quantum computers to process a vast number of possibilities at once.

## Applications

Quantum computers are particularly well-suited for:
- Cryptography and security
- Drug discovery and molecular modeling
- Optimization problems
- Machine learning
- Financial modeling

## Challenges

Current quantum computers face several challenges:
- Quantum decoherence
- Error rates
- Scalability
- Temperature requirements (near absolute zero)

## Current State

As of 2024, companies like IBM, Google, and others have demonstrated quantum computers with 100+ qubits, but practical quantum advantage for real-world problems is still being developed.
```

## Personal Data - Примеры

### tests/fixtures/personal-data/notes/trip_to_paris_2024.md

```markdown
---
title: "Trip to Paris 2024"
created_at: "2024-03-15T10:00:00Z"
tags: ["travel", "paris", "vacation"]
privacy_level: "private"
---

# Trip to Paris 2024

## March 16 - Arrival

Arrived in Paris early morning. Hotel near Gare du Nord is perfect.

## March 16 - Eiffel Tower

Visited Eiffel Tower in the afternoon. The queue was long but worth it. Amazing view from the top! Weather was perfect - sunny and warm.

Took lots of photos. The sunset view was spectacular.

## March 17 - Louvre Museum

Spent the whole day at the Louvre. Saw the Mona Lisa - was smaller than I expected but still impressive. The Egyptian collection was my favorite part.

The museum is HUGE. Could spend days there and still not see everything.

## March 17 - Dinner

Had dinner at a small restaurant near Notre-Dame. The food was excellent. Had coq au vin and crème brûlée. Very authentic French cuisine.

## March 18 - Versailles

Day trip to the Palace of Versailles. The gardens are incredible. Spent hours just walking around.

## Notes

- Need to learn more French for next time
- Want to visit Montmartre next trip
- Should plan at least 5 days to really see Paris properly
```

### tests/fixtures/personal-data/notes/quantum_computing_research.md

```markdown
---
title: "Quantum Computing Research Notes"
created_at: "2024-02-20T15:30:00Z"
tags: ["research", "quantum", "technology"]
privacy_level: "internal"
---

# Quantum Computing Research

## Key Concepts to Study

1. **Qubits and Superposition**
   - Unlike classical bits (0 or 1), qubits can be both
   - Allows parallel computation

2. **Entanglement**
   - Quantum particles can be correlated
   - Changing one affects the other instantly
   - Einstein called it "spooky action at a distance"

3. **Quantum Gates**
   - Similar to logic gates in classical computing
   - Hadamard gate creates superposition
   - CNOT gate creates entanglement

## Companies to Watch

- IBM - IBM Quantum Experience, 100+ qubit systems
- Google - Achieved quantum supremacy in 2019
- IonQ - Trapped ion technology
- Rigetti - Superconducting qubits

## Potential Applications for My Work

Could quantum computing help with:
- Machine learning model optimization?
- Database query optimization?
- Cryptography for our security system?

## Questions

- How soon until practical quantum computers?
- Cost vs classical computing?
- What problems are best suited for quantum?

## Resources

- Book: "Quantum Computing for Computer Scientists"
- Online course: IBM Quantum Learning
- Papers to read: Shor's algorithm, Grover's algorithm
```

### tests/fixtures/personal-data/chats/friends_paris_trip.json

```json
{
  "conversation_id": "chat_001",
  "conversation_name": "Paris Trip Planning",
  "platform": "telegram",
  "participants": ["Alice", "Bob", "Charlie"],
  "created_at": "2024-03-10T14:30:00Z",
  "privacy_level": "private",
  "messages": [
    {
      "id": 1,
      "from": "Alice",
      "timestamp": "2024-03-10T14:30:00Z",
      "text": "Hey, we should plan our Paris trip!"
    },
    {
      "id": 2,
      "from": "Bob",
      "timestamp": "2024-03-10T14:32:00Z",
      "text": "Yes! I've always wanted to see the Eiffel Tower"
    },
    {
      "id": 3,
      "from": "Charlie",
      "timestamp": "2024-03-10T14:35:00Z",
      "text": "We should also visit the Louvre. I heard the Egyptian collection is amazing"
    },
    {
      "id": 4,
      "from": "Alice",
      "timestamp": "2024-03-10T14:40:00Z",
      "text": "Let's go in mid-March, weather should be nice"
    },
    {
      "id": 5,
      "from": "Bob",
      "timestamp": "2024-03-10T15:00:00Z",
      "text": "I can get time off work March 15-20"
    },
    {
      "id": 6,
      "from": "Charlie",
      "timestamp": "2024-03-10T15:05:00Z",
      "text": "Perfect! That works for me too"
    },
    {
      "id": 7,
      "from": "Alice",
      "timestamp": "2024-03-10T15:10:00Z",
      "text": "Great! I'll look for hotels near the center"
    },
    {
      "id": 8,
      "from": "Bob",
      "timestamp": "2024-03-12T10:20:00Z",
      "text": "Found cheap flights! €150 return from our city"
    },
    {
      "id": 9,
      "from": "Alice",
      "timestamp": "2024-03-12T10:25:00Z",
      "text": "That's a great deal! Book it!"
    },
    {
      "id": 10,
      "from": "Charlie",
      "timestamp": "2024-03-12T11:00:00Z",
      "text": "Booked! So excited!"
    }
  ]
}
```

### tests/fixtures/personal-data/chats/work_team_chat.json

```json
{
  "conversation_id": "chat_002",
  "conversation_name": "Dev Team - Project Discussion",
  "platform": "slack",
  "participants": ["Sarah", "Mike", "David"],
  "created_at": "2024-02-15T09:00:00Z",
  "privacy_level": "private",
  "messages": [
    {
      "id": 1,
      "from": "Sarah",
      "timestamp": "2024-02-15T09:00:00Z",
      "text": "Morning team! Let's discuss the quantum computing integration project"
    },
    {
      "id": 2,
      "from": "Mike",
      "timestamp": "2024-02-15T09:02:00Z",
      "text": "I've been researching IBM Quantum and Google's quantum APIs"
    },
    {
      "id": 3,
      "from": "David",
      "timestamp": "2024-02-15T09:05:00Z",
      "text": "Do we really need quantum computing for this? Seems overkill"
    },
    {
      "id": 4,
      "from": "Sarah",
      "timestamp": "2024-02-15T09:10:00Z",
      "text": "It's more for future-proofing. We want to be ready when quantum becomes practical"
    },
    {
      "id": 5,
      "from": "Mike",
      "timestamp": "2024-02-15T09:15:00Z",
      "text": "I think we should start with a small POC. Test optimization algorithms"
    },
    {
      "id": 6,
      "from": "David",
      "timestamp": "2024-02-15T09:20:00Z",
      "text": "Fair enough. What's the timeline?"
    },
    {
      "id": 7,
      "from": "Sarah",
      "timestamp": "2024-02-15T09:25:00Z",
      "text": "Let's aim for a demo by end of Q1. Nothing production, just exploring"
    }
  ]
}
```

### tests/fixtures/personal-data/photos/photo_metadata.json

```json
[
  {
    "filename": "IMG_0001.jpg",
    "timestamp": "2024-03-16T15:30:00Z",
    "latitude": 48.858370,
    "longitude": 2.294481,
    "location_name": "Eiffel Tower",
    "camera": "iPhone 15 Pro",
    "lens": "Main 24mm f/1.78",
    "iso": 100,
    "shutter_speed": "1/500",
    "description": "View from Eiffel Tower, second level",
    "tags": ["travel", "paris", "landmark"]
  },
  {
    "filename": "IMG_0002.jpg",
    "timestamp": "2024-03-17T11:20:00Z",
    "latitude": 48.860611,
    "longitude": 2.337644,
    "location_name": "Louvre Museum",
    "camera": "iPhone 15 Pro",
    "lens": "Main 24mm f/1.78",
    "iso": 320,
    "shutter_speed": "1/60",
    "description": "Mona Lisa painting, crowded gallery",
    "tags": ["museum", "art", "paris"]
  },
  {
    "filename": "IMG_0003.jpg",
    "timestamp": "2024-03-17T16:45:00Z",
    "latitude": 48.853000,
    "longitude": 2.349900,
    "location_name": "Notre-Dame area",
    "camera": "iPhone 15 Pro",
    "lens": "Ultra Wide 13mm f/2.2",
    "iso": 200,
    "shutter_speed": "1/125",
    "description": "Restaurant near Notre-Dame, dinner time",
    "tags": ["food", "paris", "restaurant"]
  },
  {
    "filename": "IMG_0004.jpg",
    "timestamp": "2024-03-18T10:15:00Z",
    "latitude": 48.804865,
    "longitude": 2.120355,
    "location_name": "Palace of Versailles",
    "camera": "iPhone 15 Pro",
    "lens": "Main 24mm f/1.78",
    "iso": 64,
    "shutter_speed": "1/1000",
    "description": "Versailles gardens, fountain view",
    "tags": ["versailles", "gardens", "palace"]
  },
  {
    "filename": "IMG_0005.jpg",
    "timestamp": "2024-03-18T14:30:00Z",
    "latitude": 48.804865,
    "longitude": 2.120355,
    "location_name": "Palace of Versailles",
    "camera": "iPhone 15 Pro",
    "lens": "Telephoto 77mm f/2.8",
    "iso": 100,
    "shutter_speed": "1/250",
    "description": "Hall of Mirrors interior",
    "tags": ["versailles", "palace", "interior", "architecture"]
  }
]
```

## Генерация тестовых данных

Для создания полного набора test fixtures можно использовать скрипт:

```bash
#!/bin/bash
# generate_fixtures.sh

FIXTURES_DIR="tests/fixtures"

# Создать структуру директорий
mkdir -p "$FIXTURES_DIR/wikipedia-mini/Geography"
mkdir -p "$FIXTURES_DIR/wikipedia-mini/Science"
mkdir -p "$FIXTURES_DIR/wikipedia-mini/History"
mkdir -p "$FIXTURES_DIR/wikipedia-mini/Technology"
mkdir -p "$FIXTURES_DIR/personal-data/notes"
mkdir -p "$FIXTURES_DIR/personal-data/chats"
mkdir -p "$FIXTURES_DIR/personal-data/photos"

# Скопировать примеры (или генерировать программно)
# TODO: Автоматическая генерация из шаблонов

echo "Test fixtures structure created in $FIXTURES_DIR"
```

## Размеры данных

**Wikipedia Mini:**
- 20 статей × ~100 KB каждая = ~2 MB raw markdown
- После zstd compression: ~800 KB
- После chunking: ~150 chunks

**Personal Data:**
- 10 заметок × ~2-5 KB = ~30 KB
- 3 чата × ~5 KB = ~15 KB
- 5 фото metadata × ~0.5 KB = ~2.5 KB
- Total: ~50 KB

**Total test data:** ~2 MB (быстро для CI/CD)
