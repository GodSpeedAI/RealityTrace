# Canonical Evaluation Protocol

### Make sure two systems mean the same thing before comparing their results.

An evaluation says the agent succeeded.

Another says it failed.

A benchmark gives it an 87.

CI is green.

The deployment still went wrong.

Everyone has evidence.

The harder question is whether they were evaluating the same claim.

One system may mean:

```text id="mu5x8b"
success
= command returned zero
```

Another:

```text id="5iuuoq"
success
= required artifact exists
```

Another:

```text id="kfjg79"
success
= observed consequence matched the declared outcome
```

And another may quietly turn:

```text id="jbqw55"
one successful attempt
```

into:

```text id="nm6ovn"
capability
```

Those results cannot be safely compared until the distinctions underneath them agree.

**Canonical Evaluation Protocol (CEP) is a semantic evaluation and conformance protocol for represented transformations. It defines how questions, states, transformations, evidence, settlement, benchmarks, results, and semantic envelopes preserve enough meaning and provenance for independent systems to evaluate work consistently.**

Plainly:

> CEP makes sure two systems using words like `evidence`, `settled`, `result`, or `capability` are actually talking about compatible things.

CEP is vendor-neutral.

It is not an agent framework.

It is not a leaderboard.

It is not the authority layer.

It does not make something true because the data fits a CEP schema.

Its job is narrower:

```text id="no1s6p"
preserve the distinctions
        ↓
transport them without semantic loss
        ↓
test whether implementations preserve them too
```

---

## The problem is semantic collapse

Most evaluation errors are not arithmetic errors.

They happen earlier.

Two things that should remain distinct get treated as equivalent.

For example:

```text id="bdxwnp"
output
=
evidence
```

or:

```text id="j5e4uz"
completion
=
settlement
```

or:

```text id="q47jxl"
policy text
=
authority
```

or:

```text id="dboymq"
tool access
=
capability
```

or:

```text id="u2f3js"
benchmark score
=
production readiness
```

Once that collapse happens, the evaluator can be perfectly consistent and still answer the wrong question.

CEP is designed to catch the collapse before the score gets mistaken for the thing it was supposed to measure.

---

## A simple example

Suppose an agent is asked to fix a checkout regression.

You might record:

```text id="15g5an"
Declared state:
checkout regression exists

Transformation:
apply candidate fix

Observed state:
regression test passes

Evidence:
test report

Question:
did the declared checkout regression stop reproducing?

Settlement:
accepted
```

That is already more precise than:

```text id="ec078n"
task_status: success
```

Now suppose someone asks:

> Does this prove the agent has checkout-debugging capability?

CEP should preserve enough structure to answer:

```text id="iyptwy"
No.

This settlement supports one transformation result.

A capability claim requires repeated settlement
under relevant variation, transfer, and recovery.
```

The protocol protects that distinction.

It does not make the capability decision merely because the record contains the word `capability`.

---

## Start by running the proof suite

CEP is executable.

Install the repository dependencies:

```bash id="4pgspu"
uv sync
```

Run the local proof:

```bash id="m5v76y"
just all
```

That exercises the current protocol implementation by:

```text id="5fgwdd"
validating the Markdown and schema registry

validating benchmark corpus artifacts

running the test suite

running anti-collapse conformance

scoring the benchmark example answers

writing benchmark reports
```

Generated benchmark reports appear under:

```text id="7vq8w1"
reports/<family>/
```

The repository is still an early protocol implementation, so the proof suite matters more than a broad claim in this README.

---

## CEP evaluates represented transformations

The basic object is not an isolated answer.

It is a transformation situated between represented states.

```text id="kqzf2i"
declared state
      ↓
transformation
      ↓
observed state
      ↓
question
      ↓
evidence
      ↓
settlement
```

Each layer answers a different question.

### State

What condition is being represented?

### Transformation

What change was attempted?

### Observation

What was registered from a particular perspective?

### Question

What are we actually trying to determine?

### Evidence

What representation changes confidence in the answer to that question?

### Settlement

What consequence was registered strongly enough to support an update, correction, or further evaluation?

Keeping those objects separate is more important than making the envelope small.

---

## Evidence only makes sense relative to a question

Consider this:

```text id="xdmnuq"
cargo test
exit 0
```

Is that good evidence?

For this question:

> Did `cargo test` complete successfully?

Yes.

For this question:

> Was the release authorized?

No.

For this one:

> Has the team acquired durable release-engineering capability?

Also no.

The observation did not change.

The question did.

CEP therefore binds evidence to the question it is being used to answer.

That prevents evidence from gaining stronger meaning merely because it is impressive.

---

## Settlement is not just success

A system can produce several independent judgments about one transformation.

For example:

```text id="jknp8c"
execution:
successful

authority:
denied

settlement:
rejected

capability evidence:
insufficient
```

Those states are not contradictory.

They answer different questions.

A command may execute successfully while the requested outcome fails.

A transformation may produce the requested outcome while violating authority.

A single accepted result may still provide insufficient evidence for a capability claim.

CEP preserves those distinctions instead of forcing everything into:

```text id="higdya"
success = true
```

---

## Capability is deliberately expensive to claim

CEP treats capability as stronger than:

```text id="beaf00"
access

explanation

imitation

one successful attempt

one passing benchmark
```

A capability claim requires evidence from repeated settlement under relevant variation.

Depending on the evaluation contract, that may include:

```text id="r0qxs3"
repeat performance
variation coverage
transfer
reduced orchestration burden
recovery after failure
continued evidence quality
```

A CEP benchmark can evaluate whether the supplied record supports that claim.

CEP conformance itself does not grant the capability.

The protocol preserves the semantics required for another system to make that judgment consistently.

---

## The Semantic Envelope

The **Semantic Envelope** is CEP's transport object.

It carries enough context for an evaluation artifact to move between systems without reducing everything to a score and an ID.

An envelope may include:

```text id="osb3xs"
representations

states

transformations

observations

questions

claims

evidence

settlements

authority context

benchmark results

conformance reports

uncertainty

omissions

contradictions

provenance
```

The envelope is not truth.

It is a structured account of what a system claims to know and why.

That boundary matters.

A perfectly valid envelope can contain:

```text id="q55ki8"
bad evidence

an incorrect declaration

an unresolved contradiction

a rejected settlement

an unsupported claim
```

Schema validity establishes structure.

It does not establish reality.

---

## Why portability requires more than JSON

Two systems can both emit JSON containing:

```json id="4kx6g1"
{
  "status": "passed",
  "evidence": true
}
```

and still mean completely different things.

Portable evaluation needs semantics such as:

```text id="10uzz0"
what was evaluated?

against which question?

what was expected?

what was observed?

who produced the evidence?

what does "passed" mean?

which uncertainty remains?

which stronger claims are explicitly not supported?
```

CEP's semantic envelope exists to carry those distinctions across tool boundaries.

That is the interoperability problem CEP is solving.

---

## Conformance checks meaning, not just shape

A CEP-compatible system should do more than parse CEP-shaped documents.

It should preserve the protocol distinctions.

CEP therefore includes **anti-collapse conformance** tests.

Run:

```bash id="x5cx6b"
cep conformance anti-collapse
```

The detector evaluates submitted content rather than trusting a fixture's own declared verdict.

Current fixtures cover collapses such as:

```text id="iqjgok"
declared state → observed state

claim → evidence

unknown → false

completion → settlement

CI pass → peer approval

possibility → affordance

authority → capability

score → capability
```

The important property is adversarial.

A system should fail conformance when it produces a fluent answer that violates the semantic distinction.

---

## Unknown is not false

Consider:

```text id="w47qvy"
Question:
Was unauthorized access impossible?

Evidence:
No unauthorized-access event was found.
```

The correct evaluation may be:

```text id="fru71q"
unknown
```

rather than:

```text id="g0cdxc"
false
```

or:

```text id="r41rqb"
confirmed safe
```

The search scope may have been incomplete.

The relevant observation may not exist.

The detector may not have had access to every path.

CEP preserves uncertainty and omission explicitly so missing evidence cannot quietly become evidence of absence.

---

## Four executable benchmark families

CEP currently ships four benchmark families.

They exercise different semantic boundaries.

### RealityGapBench

Asks:

> Did declared reality diverge from observed reality?

Cases can cover areas such as:

```text id="bracbo"
authority drift
process drift
evidence gaps
settlement gaps
semantic drift
affordance gaps
capability gaps
```

The scorer evaluates whether the answer detects and classifies the discrepancy and supports it with appropriate evidence.

---

### AuthorityBench

Asks:

> Given this actor, operation, resource, context, and policy, what is the authority decision?

Expected decision space:

```text id="4q7m83"
allow
deny
escalate
```

The benchmark prevents a common substitution:

```text id="qaosve"
the agent can do this
```

standing in for:

```text id="k3n3ka"
the agent is allowed to do this
```

Capability is not authority.

---

### SettlementBench

Asks:

> Did the completion claim actually settle?

A completion summary cannot settle itself.

The evaluation must consider the declared question, evidence, consequence, and applicable settlement criteria.

This benchmark targets premature completion.

---

### CapabilityBench

Asks:

> Does the available settlement history support the capability claim?

A high score, one successful task, or tool access cannot substitute for the required developmental evidence.

This benchmark targets imaginary capability.

---

## The current corpus is intentionally small

The four benchmark families are executable today.

The corpus is not yet large.

It currently contains roughly one to three reviewed cases per family.

That makes it a **smoke corpus**.

It is sufficient to test:

```text id="ke8u5g"
schemas

runner behavior

scoring

anti-collapse logic

report generation

end-to-end execution
```

It is not sufficient to make broad empirical claims about model quality across domains.

Growing the reviewed case corpus is the main roadmap item.

See:

* [Authoring Benchmark Cases](docs/authoring-cases.md)

---

## Benchmark results are evidence about the benchmark

This sounds obvious and is worth stating explicitly.

A model scoring:

```text id="5kpx9u"
92
```

on a CEP benchmark establishes something about its performance on that benchmark under that evaluation contract.

It does not automatically establish:

```text id="35aa55"
production readiness

organizational capability

authorization

safety in another domain

transfer to untested environments
```

Those stronger conclusions require their own evidence.

CEP exists partly to make that escalation of claim visible.

---

## The ontology is derived, not an arbitrary field list

CEP's specification organizes its concepts through a derivation ladder:

```text id="zljguh"
0. Indeterminate Field
1. Constraint
2. Distinction
3. Relation
4. Representation
5. State
6. Transformation
7. Observation
8. Question
9. Evidence
10. Settlement
```

The ladder is mainly useful for protocol design.

It forces later concepts to preserve distinctions required by earlier ones.

For example:

```text id="8jkksl"
state
```

requires a representation situated in context.

```text id="mtknj2"
transformation
```

requires something capable of changing between represented states.

```text id="n26gac"
evidence
```

requires a question relative to which the representation matters.

```text id="buysrz"
settlement
```

requires consequence that can update the evaluation state.

Most users do not need to reason from level zero to use CEP.

Protocol authors do.

---

## Specifications

The initial specification family includes:

```text id="berwlm"
CEP-0000  Derivation Ladder and First Principles
CEP-0001  Ontology
CEP-0002  Representation Model
CEP-0003  State Model
CEP-0004  Transformation Model
CEP-0005  Question Model
CEP-0006  Evidence Model
CEP-0007  Settlement Model
CEP-0008  Semantic Envelope
CEP-0009  Benchmark Model
CEP-0010  Conformance
```

Start with:

* [`spec/`](spec/index.md)

The specification is the semantic authority.

The reference implementation exists to exercise it.

---

## Repository structure

```text id="rram93"
.
├── spec/
│   └── normative CEP specifications
│
├── schemas/
│   └── schemas for protocol records
│
├── benchmarks/
│   └── benchmark family definitions and cases
│
├── conformance/
│   └── anti-collapse fixtures
│
├── src/cep/
│   └── reference CLI, validators, scorers, detector
│
├── tests/
│   └── implementation and scoring tests
│
├── reports/
│   └── generated benchmark results
│
├── rfc/
│   └── proposed protocol changes
│
└── docs/
    └── guides and reference material
```

---

## The reference implementation

CEP currently includes a local reference implementation with:

```text id="o3hsm5"
CLI

schema validation

benchmark scorers

anti-collapse detector

benchmark runner

result generation
```

The implementation demonstrates that the protocol is executable.

It is not the definition of CEP.

Another implementation can conform without using the same codebase, language, storage, model provider, or scoring infrastructure.

That is the point of having a protocol.

---

## Protocol compatibility should survive implementation changes

Imagine:

```text id="u94625"
System A
uses Python

System B
uses Rust

System C
uses a hosted evaluation platform
```

If all three preserve the same CEP semantics, their envelopes and evaluation artifacts should remain interpretable across the boundary.

CEP therefore aims for interoperability at the semantic level rather than requiring one runtime.

The test is not:

> Did you use our library?

It is:

> Did your implementation preserve the distinctions the protocol requires?

---

## CEP and RealityTrace

RealityTrace is one producer and consumer of CEP-shaped evaluation records.

RealityTrace asks:

> What did we expect, what did we observe, and what difference remains?

CEP defines portable semantics for carrying objects such as:

```text id="3kx2li"
declared state
observed state
evidence
settlement
provenance
```

between that system and other evaluators.

The relationship is:

```text id="360pvc"
RealityTrace
produces evidence and discrepancy records
        ↓
CEP Semantic Envelope
preserves their evaluation semantics
        ↓
another evaluator
can inspect them without guessing
what the fields meant
```

CEP does not need to become RealityTrace.

RealityTrace does not need to become the protocol.

---

## CEP and SEA-Forge

SEA-Forge can produce authority and operational evidence.

CEP can represent that evidence and evaluate whether an authority claim preserves the distinction between:

```text id="gs44ay"
can act
```

and:

```text id="wvvcbs"
may act
```

AuthorityBench exercises that distinction.

CEP does not grant runtime authority.

SEA-Forge owns that operational boundary.

---

## CEP and SWE_SEED

SWE_SEED can produce software-work records containing:

```text id="sdcyq8"
route
artifact
proof
trace
```

CEP can transport or evaluate claims built from that evidence.

For example:

```text id="khg8x9"
SWE_SEED:
this proof command ran and passed

CEP:
what question does that evidence support,
and what stronger claims does it not support?
```

Again, the protocol preserves semantics rather than replacing the producer.

---

## CEP and GodSpeed-Agent

GodSpeed-Agent evaluates developmental history to decide what repeated settlement justifies about future capability.

CEP can provide a portable representation for the evidence and evaluation claims involved.

It should not collapse:

```text id="2a7wgc"
CEP-conformant capability record
```

into:

```text id="5w35hx"
capability established
```

Conformance establishes semantic preservation.

The developmental controller still has to evaluate the underlying evidence.

---

## Conformance does not establish truth

This boundary deserves its own section.

A record can be:

```text id="kfskrb"
valid CEP
```

and still contain:

```text id="c7mg81"
an incorrect expectation

weak evidence

a bad measurement

a misleading benchmark

a false claim

an unresolved contradiction
```

CEP conformance establishes something narrower:

> The implementation preserved the semantic distinctions required by the declared protocol scope.

That is valuable because it lets systems disagree honestly.

It does not make disagreement disappear.

---

## What CEP is not

### Not an evaluation model

CEP does not prescribe one LLM judge.

Different evaluators can operate under the same protocol.

### Not a benchmark leaderboard

CEP defines benchmark families and a reference runner.

The protocol is larger than a ranking table.

### Not runtime authority

A CEP AuthorityBench answer can evaluate an authority decision.

It cannot authorize the real-world side effect.

### Not evidence collection

CEP defines evidence semantics.

Systems such as RealityTrace collect and bind operational evidence.

### Not settlement reality

CEP can represent and score settlement under an evaluation contract.

It cannot guarantee that the declared observation captured all relevant consequence in the world.

### Not capability formation

CEP can evaluate whether evidence supports a capability claim.

It does not create capability by returning a passing score.

### Not a governance dashboard

CEP is designed to sit beneath many possible interfaces and systems.

---

## Design principles

CEP follows a few rules consistently.

### Preserve distinctions

Do not collapse:

```text id="ve9zbv"
claim
evidence
settlement
authority
capability
affordance
```

because they are inconvenient to model separately.

### Bind evidence to questions

Evidence has meaning relative to what is being asked.

### Preserve provenance

An evaluator should be able to determine where important evaluation artifacts came from.

### Treat outcomes as settlement events

An outcome matters because consequence can update the system.

It is not automatically the terminal purpose of the system.

### Keep uncertainty explicit

Unknown is not false.

Missing evidence is not proof.

### Make compatibility testable

Protocol conformance should be exercised with fixtures.

It should not be a marketing claim.

---

## Who CEP is for

CEP becomes useful when several systems need to exchange evaluation claims without inventing private definitions for the same words.

That includes:

* AI evaluation researchers
* agent framework builders
* benchmark designers
* AI reliability engineers
* governance and assurance teams
* enterprise architecture teams
* compliance and audit systems
* human-AI workflow designers
* organizations building governed agent systems

If one small application owns the entire evaluation loop and no evaluation artifacts need to cross a system boundary, adopting a protocol may be unnecessary.

Use CEP when semantic interoperability is actually a problem.

---

## When CEP is probably worth trying

CEP starts to earn its cost when:

* two evaluation systems disagree because they define success differently
* benchmark results need to travel between tools
* authority, settlement, and capability claims keep being conflated
* several products emit evidence that one downstream evaluator must interpret
* missing evidence is routinely collapsed into pass or fail
* benchmark scores are being promoted into operational claims they do not support
* you need vendor-neutral evaluation records
* protocol compatibility needs to be tested rather than asserted

A protocol should reduce semantic translation cost.

If it adds more vocabulary than the systems actually need to exchange, it has not earned its complexity.

---

## Current status

CEP is a draft protocol with an executable reference implementation.

Implemented today:

```text id="25xa6q"
specification set

JSON schemas

four benchmark families

benchmark scorers

semantic envelopes

anti-collapse fixtures

content-based conformance detector

CLI

local proof suite
```

The benchmark corpus remains small.

Treat the current implementation as proof that the architecture runs end to end, not as proof that the benchmark suite already covers the full evaluation space.

Expect breaking changes while the initial specification family settles.

---

## Documentation

* [Specifications](spec/index.md)
* [Running the Benchmarks](docs/running-benchmarks.md)
* [Authoring Benchmark Cases](docs/authoring-cases.md)
* [Anti-Collapse Reference](docs/anti-collapse.md)
* [Settlement Scoring Call Surface](docs/settlement-call-surface.md)

---

## The short version

Before comparing evaluation results, preserve what each result actually means.

Keep declared state separate from observed state.

Keep claims separate from evidence.

Bind evidence to the question it answers.

Keep completion separate from settlement.

Keep authority separate from capability.

Keep one successful attempt separate from reliable capability.

Keep uncertainty visible.

Move those distinctions between systems in a semantic envelope.

Test whether implementations preserve them.

Do not confuse conformance with truth.

That is CEP.

---

## License

Apache 2.0

---

## Project status

Draft protocol.

Expect breaking changes until the initial specification set, schemas, fixtures, and reference implementation are settled.
