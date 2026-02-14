# SEMANTIC v2 — Arquitetura Completa

## Biblioteca Rust de Representação de Conhecimento Distribuída
### Epistêmica + Hyperlinks N-ários + Merkle-CRDT

---

## 1. VISÃO GERAL

**Semantic** é uma biblioteca Rust para representação de conhecimento vivo e distribuído.
Não é um grafo estático — é um sistema epistêmico onde conhecimento tem graus de verdade,
energia de ativação, e capacidade de sincronização entre réplicas sem coordenação central.

### 1.1 Os 6 Domínios Teóricos (mantidos da v1)

| Domínio | Contribuição para Semantic v2 |
|---|---|
| **DNA/Biologia** | Modelo de camadas (nucleotídeo→gene→cromossomo→genoma ≈ Concept→Link→Domain→KnowledgeBase), auto-reparo, decaimento |
| **NARS** | TruthValue ⟨f,c⟩, AIKR (Assumption of Insufficient Knowledge and Resources), regras de revisão/dedução/indução/abdução |
| **Sistemas Complexos** | Emergência, autopoiese, feedback loops, atratores no espaço de estados |
| **Teoria das Categorias** | Composição de morfismos (Links compõem), functores entre Domains, limites/colimites para merge |
| **Bio-Inspiração** | Energia metabólica, decaimento, regulação alostérica (um Link influencia outro), seleção natural de conhecimento |
| **Ontologias** | Hierarquia de tipos, herança, relações N-árias com papéis, frames semânticos |

### 1.2 Decisões Arquiteturais

```
┌─────────────────────────────────────────────────────────┐
│ Nomenclatura:    Epistêmica (Concept, Link, Domain...)  │
│ Modelo de dados: Hyperlinks N-ários (Concept + Link)    │
│ Distribuição:    Merkle-CRDT (content-addressed DAG)    │
│ Linguagem:       Rust (paradigma funcional prioritário)  │
│ Hash:            BLAKE3 (rápido, Merkle nativo, 256-bit) │
└─────────────────────────────────────────────────────────┘
```

---

## 2. NOMENCLATURA EPISTÊMICA — Mapeamento Completo

### De biologia para conhecimento:

| Antigo (v1) | Novo (v2) | O que é | Analogia simples |
|---|---|---|---|
| Spore | **Concept** | Unidade atômica de conhecimento | Um verbete da enciclopédia |
| Bond | **Link** | Relação N-ária entre Concepts com papéis | Uma frase: "João *comprou* livro *de* Maria" |
| Membrane | **Domain** | Fronteira/contexto que agrupa conhecimento | Um volume da enciclopédia |
| Signal | **Inference** | Propagação de mudanças e derivações | Uma nota do editor que atualiza referências cruzadas |
| Metabolism | **Attention** | Sistema de energia, decaimento e priorização | O bibliotecário decidindo o que fica na estante |
| BindingSite | **Slot** | Posição/papel num Link que um Concept pode ocupar | O espaço em branco numa frase: "__ comprou __ de __" |
| TruthValue | **TruthValue** | Grau de verdade ⟨frequency, confidence⟩ | Nota de confiança (0-100%) + quanta evidência viu |
| Evidence | **Evidence** | Registro de observação a favor ou contra | Um carimbo: "vi isso acontecer dia X" |
| Semantic | **KnowledgeBase** | Container raiz: todos os Domains + DAG Merkle | A biblioteca inteira |

---

## 3. MODELO DE DADOS — Hyperlinks N-ários

### 3.1 Concept (Unidade Atômica de Conhecimento)

```rust
// Versão: conceitual — tipos finais no módulo de implementação

/// Identificador único, content-addressed
pub type ConceptId = ContentHash; // BLAKE3 do conteúdo canônico

/// Unidade atômica de conhecimento
pub struct Concept {
    /// Hash BLAKE3 do (label + domain_id + created_by)
    pub id: ConceptId,
    
    /// Rótulo legível ("fotossíntese", "Rust", "João")
    pub label: String,
    
    /// Grau de verdade NARS: ⟨frequency, confidence⟩
    /// frequency ∈ [0.0, 1.0] — proporção de evidência positiva
    /// confidence ∈ [0.0, 1.0) — NUNCA alcança 1.0 (AIKR)
    pub truth: TruthValue,
    
    /// Energia de ativação (0.0 = dormente, 1.0 = máximo)
    /// Decai com o tempo; reforçado por uso/evidência
    pub energy: f64,
    
    /// Estado do ciclo de vida
    pub state: ConceptState,
    
    /// Histórico de evidências (compactado)
    pub evidence: EvidenceBag,
    
    /// Em qual Domain este Concept reside
    pub domain_id: DomainId,
    
    /// Timestamp lógico (Hybrid Logical Clock)
    pub hlc: HybridTimestamp,
    
    /// Hash do nó Merkle-DAG que criou/modificou este Concept
    pub merkle_ref: MerkleNodeId,
}

/// Estados do ciclo de vida (inspirado em biologia, mas com nomes epistêmicos)
pub enum ConceptState {
    Active,      // Em uso ativo, energia > limiar
    Dormant,     // Baixa energia, mas preservado
    Fading,      // Em processo de decaimento
    Archived,    // Tombstone para CRDT (nunca removido, marcado)
}
```

### 3.2 Link (Relação N-ária com Papéis)

O **Link** é o componente mais poderoso e diferenciado da Semantic v2.
Diferente de grafos tradicionais (A→B), um Link conecta **N Concepts**,
onde cada participante tem um **papel nomeado** (Slot).

```rust
/// Identificador de Link, content-addressed
pub type LinkId = ContentHash; // BLAKE3 do (kind + participants sorted)

/// Relação N-ária entre Concepts
pub struct Link {
    /// Hash BLAKE3 do conteúdo canônico
    pub id: LinkId,
    
    /// Tipo semântico da relação
    pub kind: LinkKind,
    
    /// Participantes com seus papéis (N ≥ 2)
    /// Ordenados por role para determinismo no hash
    pub participants: Vec<Participant>,
    
    /// Grau de verdade desta relação
    pub truth: TruthValue,
    
    /// Energia de ativação
    pub energy: f64,
    
    /// Evidências que suportam/refutam esta relação
    pub evidence: EvidenceBag,
    
    /// Domain onde este Link foi criado
    pub domain_id: DomainId,
    
    /// Timestamp lógico
    pub hlc: HybridTimestamp,
    
    /// Referência no Merkle-DAG
    pub merkle_ref: MerkleNodeId,
}

/// Um participante num Link N-ário
pub struct Participant {
    /// Qual Concept participa
    pub concept_id: ConceptId,
    
    /// Qual papel ocupa nesta relação
    pub role: Role,
    
    /// Posição ordinal (para relações onde ordem importa)
    pub position: u32,
}

/// Papel semântico de um participante
/// Inspirado em frames semânticos da linguística + ontologias
pub enum Role {
    Subject,     // Agente/sujeito principal
    Object,      // Objeto/alvo
    Predicate,   // O que conecta (o "verbo")
    Source,       // Origem
    Target,       // Destino
    Instrument,   // Meio/ferramenta
    Context,      // Contexto qualificador
    Value,        // Valor/quantidade
    Qualifier,    // Modificador/qualificador
    Custom(String), // Papel customizado
}

/// Tipos de relação (extensível)
/// Combina NARS copulas + ontologias + teoria das categorias
pub enum LinkKind {
    // === NARS-inspired ===
    Inheritance,   // S → P ("Gato é um Animal")
    Similarity,    // S ↔ P ("Gato se parece com Tigre")
    Implication,   // S ⇒ P ("Se chove, rua molhada")
    Equivalence,   // S ⇔ P ("H₂O é água")
    
    // === Ontologia-inspired ===
    PartOf,        // Composição ("Motor é parte de Carro")
    HasProperty,   // Atributo ("Rosa tem cor vermelha")
    InstanceOf,    // Instanciação ("Fido é instância de Cão")
    
    // === Bio-inspired (renomeados) ===
    Catalyzes,     // A potencializa B (alostérico positivo)
    Inhibits,      // A inibe B (alostérico negativo)
    
    // === Categoria-inspired ===
    MapsTo,        // Functor entre domains
    
    // === Extensível ===
    Custom(String),
}
```

### 3.3 Exemplos de Links N-ários

```rust
// Exemplo 1: Relação simples (binária, como grafo)
// "Gato é um Animal" com confiança 0.95
Link {
    kind: Inheritance,
    participants: vec![
        Participant { concept_id: gato, role: Subject, position: 0 },
        Participant { concept_id: animal, role: Object, position: 1 },
    ],
    truth: TruthValue { frequency: 0.95, confidence: 0.87 },
    ..
}

// Exemplo 2: Relação ternária (N=3)
// "João comprou livro de Maria"
Link {
    kind: Custom("Transaction"),
    participants: vec![
        Participant { concept_id: joao, role: Subject, position: 0 },
        Participant { concept_id: livro, role: Object, position: 1 },
        Participant { concept_id: maria, role: Source, position: 2 },
    ],
    truth: TruthValue { frequency: 1.0, confidence: 0.92 },
    ..
}

// Exemplo 3: Relação quaternária (N=4) — diagnóstico médico
// "Paciente masculino com creatinina 115-133 µmol/L → leve elevação"
Link {
    kind: Implication,
    participants: vec![
        Participant { concept_id: paciente, role: Subject, position: 0 },
        Participant { concept_id: masculino, role: Qualifier, position: 1 },
        Participant { concept_id: creatinina_range, role: Context, position: 2 },
        Participant { concept_id: elevacao_leve, role: Object, position: 3 },
    ],
    truth: TruthValue { frequency: 0.88, confidence: 0.75 },
    ..
}

// Exemplo 4: Meta-link (Link sobre Link)
// "A relação 'Gato é Animal' tem alta confiança"
// O id do Link anterior é tratado como ConceptId via wrapper
Link {
    kind: HasProperty,
    participants: vec![
        Participant { concept_id: link_gato_animal.id.as_concept_ref(), role: Subject, position: 0 },
        Participant { concept_id: alta_confianca, role: Object, position: 1 },
    ],
    truth: TruthValue { frequency: 0.80, confidence: 0.60 },
    ..
}
```

### 3.4 Domain (Fronteira de Contexto)

```rust
pub type DomainId = ContentHash;

/// Agrupa Concepts e Links num contexto semântico
pub struct Domain {
    pub id: DomainId,
    
    /// Nome do domínio ("medicina", "culinária", "rust-lang")
    pub name: String,
    
    /// Domain pai (para hierarquia: "cardiologia" ⊂ "medicina")
    pub parent: Option<DomainId>,
    
    /// Configuração do sistema de Attention para este Domain
    pub attention_config: AttentionConfig,
    
    /// Permeabilidade: quão facilmente Links cruzam este Domain
    /// LinkKind → probabilidade de propagação (0.0 = bloqueado, 1.0 = livre)
    pub permeability: HashMap<LinkKind, f64>,
    
    /// Budget de energia total disponível para Concepts neste Domain
    pub energy_budget: f64,
    
    /// Referência Merkle
    pub merkle_ref: MerkleNodeId,
}
```

---

## 4. TRUTHVALUE — Sistema NARS de Verdade

### 4.1 Definição Formal

```rust
/// Grau de verdade baseado em NARS (Non-Axiomatic Logic)
/// 
/// Internamente usa evidência (w⁺, w⁻).
/// Externamente expõe (frequency, confidence).
///
/// Conversões:
///   frequency  = w⁺ / (w⁺ + w⁻)           — proporção de evidência positiva
///   confidence = (w⁺ + w⁻) / (w⁺ + w⁻ + k) — estabilidade (k = parâmetro horizon)
///   w⁺ = k × f × c / (1 - c)
///   w⁻ = k × (1-f) × c / (1 - c)
///
/// INVARIANTE: confidence < 1.0 (AIKR — nada é axioma)
#[derive(Clone, Debug)]
pub struct TruthValue {
    /// Evidência positiva (w⁺ ≥ 0)
    positive_evidence: f64,
    
    /// Evidência negativa (w⁻ ≥ 0)
    negative_evidence: f64,
}

/// Parâmetro de horizonte evidencial (default = 1.0 em NARS)
/// Controla quão rápido confidence cresce com mais evidência
const EVIDENTIAL_HORIZON: f64 = 1.0;

impl TruthValue {
    /// Cria a partir de frequency e confidence
    pub fn new(frequency: f64, confidence: f64) -> Self {
        assert!(confidence < 1.0, "AIKR: confidence nunca alcança 1.0");
        assert!((0.0..=1.0).contains(&frequency));
        assert!((0.0..1.0).contains(&confidence));
        
        let k = EVIDENTIAL_HORIZON;
        let w_total = k * confidence / (1.0 - confidence);
        Self {
            positive_evidence: w_total * frequency,
            negative_evidence: w_total * (1.0 - frequency),
        }
    }
    
    /// Frequency: proporção de evidência positiva
    pub fn frequency(&self) -> f64 {
        let total = self.positive_evidence + self.negative_evidence;
        if total == 0.0 { 0.5 } else { self.positive_evidence / total }
    }
    
    /// Confidence: estabilidade da avaliação
    pub fn confidence(&self) -> f64 {
        let total = self.positive_evidence + self.negative_evidence;
        total / (total + EVIDENTIAL_HORIZON)
    }
    
    /// Expectation: valor esperado = c × (f - 0.5) + 0.5
    pub fn expectation(&self) -> f64 {
        self.confidence() * (self.frequency() - 0.5) + 0.5
    }
}
```

### 4.2 Regra de Revisão (Merge para CRDT)

A **revisão** é a operação mais crítica para distribuição:
quando duas réplicas têm evidências independentes sobre o mesmo Concept,
a revisão combina ambas.

```rust
impl TruthValue {
    /// NARS Revision Rule — merge de evidências independentes
    ///
    /// É comutativa: revision(a, b) == revision(b, a)  ✓
    /// É associativa: revision(a, revision(b, c)) == revision(revision(a, b), c)  ✓
    /// É idempotente: revision(a, a) == a  ✓
    ///
    /// Estas 3 propriedades fazem do TruthValue um JOIN-SEMILATTICE,
    /// tornando-o automaticamente um CRDT válido.
    pub fn revision(&self, other: &TruthValue) -> TruthValue {
        TruthValue {
            positive_evidence: self.positive_evidence + other.positive_evidence,
            negative_evidence: self.negative_evidence + other.negative_evidence,
        }
    }
    
    /// NARS Deduction: S→M + M→P ⊢ S→P
    /// f = f1 × f2
    /// c = f1 × f2 × c1 × c2
    pub fn deduction(&self, other: &TruthValue) -> TruthValue {
        let f = self.frequency() * other.frequency();
        let c = self.frequency() * other.frequency() * self.confidence() * other.confidence();
        TruthValue::new(f, c.min(0.9999))
    }
    
    /// NARS Induction: M→P + M→S ⊢ S→P
    /// f = f2
    /// c = f1 × c1 × c2 / (f1 × c1 × c2 + k)
    pub fn induction(&self, other: &TruthValue) -> TruthValue {
        let f = other.frequency();
        let w = self.frequency() * self.confidence() * other.confidence();
        let c = w / (w + EVIDENTIAL_HORIZON);
        TruthValue::new(f, c.min(0.9999))
    }
    
    /// NARS Abduction: P→M + S→M ⊢ S→P
    /// f = f1
    /// c = f2 × c1 × c2 / (f2 × c1 × c2 + k)
    pub fn abduction(&self, other: &TruthValue) -> TruthValue {
        let f = self.frequency();
        let w = other.frequency() * self.confidence() * other.confidence();
        let c = w / (w + EVIDENTIAL_HORIZON);
        TruthValue::new(f, c.min(0.9999))
    }
}
```

### 4.3 Propriedade CRDT do TruthValue

```
PROVA INFORMAL de que TruthValue.revision é um join-semilattice:

1. COMUTATIVIDADE:
   (w⁺₁ + w⁺₂, w⁻₁ + w⁻₂) == (w⁺₂ + w⁺₁, w⁻₂ + w⁻₁)  ✓ (adição comutativa)

2. ASSOCIATIVIDADE:
   ((w⁺₁ + w⁺₂) + w⁺₃) == (w⁺₁ + (w⁺₂ + w⁺₃))  ✓ (adição associativa)

3. IDEMPOTÊNCIA:
   ⚠️ A adição simples NÃO é idempotente! (w⁺ + w⁺ ≠ w⁺)
   
   SOLUÇÃO: Usar evidência com rastreamento de fonte.
   Cada evidência tem um ID único (derivado do nó Merkle que a gerou).
   Ao fazer revisão, evidências com mesmo ID são deduplicadas.
   
   revision(a, b) = TruthValue {
       positive_evidence: merge_unique(a.evidence_set, b.evidence_set).count_positive(),
       negative_evidence: merge_unique(a.evidence_set, b.evidence_set).count_negative(),
   }
   
   Com deduplicação por ID: revision(a, a) == a  ✓

CONCLUSÃO: Com EvidenceBag rastreando fontes únicas,
           TruthValue.revision forma um join-semilattice válido
           → é um State-based CRDT (CmRDT) por construção.
```

---

## 5. EVIDENCEBAG — Rastreamento de Evidências

```rust
/// Uma peça individual de evidência
pub struct Evidence {
    /// Hash único desta evidência (content-addressed)
    pub id: EvidenceId,
    
    /// Positiva (+1) ou negativa (-1)
    pub polarity: Polarity,
    
    /// Fonte que gerou esta evidência (NodeId da réplica)
    pub source: ReplicaId,
    
    /// Quando foi observada
    pub timestamp: HybridTimestamp,
    
    /// Referência ao nó Merkle que registrou
    pub merkle_ref: MerkleNodeId,
}

pub enum Polarity {
    Positive,  // Evidência a favor
    Negative,  // Evidência contra
}

/// Bag de evidências com deduplicação por ID
/// Implementa um G-Set CRDT (Grow-only Set)
pub struct EvidenceBag {
    /// Set de IDs de evidência já vistos (para deduplicação)
    evidence_ids: HashSet<EvidenceId>,
    
    /// Contadores derivados (cache)
    positive_count: u64,
    negative_count: u64,
}

impl EvidenceBag {
    /// Merge de dois bags (union dos sets)
    /// É um G-Set CRDT: union é comutativa, associativa e idempotente
    pub fn merge(&self, other: &EvidenceBag) -> EvidenceBag {
        let merged_ids = self.evidence_ids.union(&other.evidence_ids).cloned().collect();
        // Recontagem necessária pois evidências duplicadas não contam 2x
        // Na prática, armazenamos o set completo e derivamos contadores
        todo!("recount from merged evidence set")
    }
}
```

---

## 6. MERKLE-CRDT — Camada de Distribuição

### 6.1 Conceito Central

```
Analogia: Pense no Merkle-CRDT como o Git para conhecimento.

No Git:
  - Cada commit tem um hash único baseado no conteúdo
  - Commits apontam para commits pais
  - Sincronizar = trocar hashes que o outro não tem
  - Merge = combinar branches divergentes
  - Sem coordenação central necessária

No Semantic v2:
  - Cada mudança (criar Concept, adicionar evidência, formar Link) é um nó no DAG
  - Cada nó tem hash BLAKE3 do conteúdo + referências aos pais
  - Sincronizar = comparar heads, buscar nós faltantes
  - Merge = aplicar payloads CRDT (TruthValue.revision, EvidenceBag.merge)
  - Sem coordenação central necessária
```

### 6.2 Estrutura do Merkle-DAG

```rust
/// Hash de conteúdo BLAKE3 (32 bytes = 256 bits)
pub type ContentHash = [u8; 32];
pub type MerkleNodeId = ContentHash;

/// Um nó no Merkle-DAG
pub struct MerkleNode {
    /// Hash BLAKE3 de (payload_hash + parents_sorted + hlc)
    /// Este campo é derivado, não armazenado
    pub id: MerkleNodeId,
    
    /// Nós pais no DAG (hashes dos antecessores causais)
    pub parents: Vec<MerkleNodeId>,
    
    /// A mudança que este nó representa
    pub payload: MerklePayload,
    
    /// Timestamp lógico (Hybrid Logical Clock)
    pub hlc: HybridTimestamp,
    
    /// Quem criou este nó
    pub author: ReplicaId,
}

/// Tipos de payload que podem ser transportados no DAG
pub enum MerklePayload {
    /// Criação de um novo Concept
    ConceptCreated {
        concept: Concept,
    },
    
    /// Adição de evidência a um Concept existente
    EvidenceAdded {
        concept_id: ConceptId,
        evidence: Evidence,
    },
    
    /// Formação de um novo Link
    LinkFormed {
        link: Link,
    },
    
    /// Atualização de energia (decaimento ou reforço)
    EnergyUpdated {
        target_id: ContentHash, // Concept ou Link
        new_energy: f64,
    },
    
    /// Arquivamento (tombstone para CRDT)
    Archived {
        target_id: ContentHash,
        reason: String,
    },
    
    /// Criação/modificação de Domain
    DomainChanged {
        domain: Domain,
    },
}

impl MerkleNode {
    /// Calcula o hash deste nó (determinístico)
    pub fn compute_id(&self) -> MerkleNodeId {
        let mut hasher = blake3::Hasher::new();
        
        // Hash do payload
        hasher.update(&self.payload.canonical_bytes());
        
        // Parents ordenados (determinismo)
        let mut sorted_parents = self.parents.clone();
        sorted_parents.sort();
        for parent in &sorted_parents {
            hasher.update(parent);
        }
        
        // Timestamp
        hasher.update(&self.hlc.to_bytes());
        
        // Author
        hasher.update(&self.author.to_bytes());
        
        *hasher.finalize().as_bytes()
    }
}
```

### 6.3 Protocolo de Sincronização

```rust
/// Estado de sincronização de uma réplica
pub struct SyncState {
    /// Heads atuais do DAG (nós sem filhos)
    pub heads: HashSet<MerkleNodeId>,
    
    /// Conjunto de todos os nós conhecidos
    pub known_nodes: HashSet<MerkleNodeId>,
}

/// Protocolo de Anti-Entropy para Merkle-CRDT
/// Baseado no paper "Merkle-CRDTs: Merkle-DAGs meet CRDTs"
/// (Sanjuán, Pöyhtäri, Teixeira, 2020)
pub trait SyncProtocol {
    /// 1. Réplica A envia suas heads para B
    fn advertise_heads(&self) -> Vec<MerkleNodeId>;
    
    /// 2. B compara com suas heads e identifica nós faltantes
    fn compute_missing(&self, remote_heads: &[MerkleNodeId]) -> Vec<MerkleNodeId>;
    
    /// 3. A envia os nós que B não tem (caminhando o DAG)
    fn fetch_nodes(&self, missing: &[MerkleNodeId]) -> Vec<MerkleNode>;
    
    /// 4. B aplica os nós recebidos, fazendo merge dos payloads CRDT
    fn apply_nodes(&mut self, nodes: Vec<MerkleNode>) -> Result<(), SyncError>;
    
    /// 5. Verificação: ambas réplicas convergem para mesmo estado
    fn verify_convergence(&self, other_heads: &[MerkleNodeId]) -> bool;
}

/// Algoritmo de sync simplificado:
///
/// fn sync(replica_a, replica_b):
///     heads_a = replica_a.advertise_heads()
///     heads_b = replica_b.advertise_heads()
///     
///     // A busca o que B tem e A não
///     missing_a = replica_a.compute_missing(heads_b)
///     nodes_for_a = replica_b.fetch_nodes(missing_a)
///     replica_a.apply_nodes(nodes_for_a)
///     
///     // B busca o que A tem e B não
///     missing_b = replica_b.compute_missing(heads_a)
///     nodes_for_b = replica_a.fetch_nodes(missing_b)
///     replica_b.apply_nodes(nodes_for_b)
///     
///     // Ambos agora têm o mesmo DAG e convergem
///     assert!(replica_a.state == replica_b.state)
```

### 6.4 Hybrid Logical Clock (HLC)

```rust
/// Hybrid Logical Clock — combina relógio físico com lógico
/// Baseado em Kulkarni et al. (2014)
/// 
/// Garante:
/// - Se evento A causou B, então hlc(A) < hlc(B)
/// - Timestamps são monotonicamente crescentes por réplica
/// - Aproximação do tempo real (para debugging e visualização)
pub struct HybridTimestamp {
    /// Componente de tempo físico (millis desde epoch)
    pub wall_time: u64,
    
    /// Componente lógico (para desempate quando wall_time é igual)
    pub logical: u32,
    
    /// ID da réplica que gerou (para desempate total)
    pub replica_id: ReplicaId,
}

impl HybridTimestamp {
    /// Gera novo timestamp para evento local
    pub fn now(replica_id: ReplicaId, last: &HybridTimestamp) -> Self {
        let physical = current_time_millis();
        let wall_time = physical.max(last.wall_time);
        let logical = if wall_time == last.wall_time {
            last.logical + 1
        } else {
            0
        };
        Self { wall_time, logical, replica_id }
    }
    
    /// Atualiza ao receber mensagem remota
    pub fn receive(
        replica_id: ReplicaId,
        local_last: &HybridTimestamp,
        remote: &HybridTimestamp,
    ) -> Self {
        let physical = current_time_millis();
        let wall_time = physical.max(local_last.wall_time).max(remote.wall_time);
        let logical = if wall_time == local_last.wall_time && wall_time == remote.wall_time {
            local_last.logical.max(remote.logical) + 1
        } else if wall_time == local_last.wall_time {
            local_last.logical + 1
        } else if wall_time == remote.wall_time {
            remote.logical + 1
        } else {
            0
        };
        Self { wall_time, logical, replica_id }
    }
}

impl Ord for HybridTimestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.wall_time.cmp(&other.wall_time)
            .then(self.logical.cmp(&other.logical))
            .then(self.replica_id.cmp(&other.replica_id))
    }
}
```

---

## 7. ATTENTION — Sistema de Energia e Decaimento

### 7.1 Modelo de Energia

```rust
/// Configuração do sistema de Attention para um Domain
pub struct AttentionConfig {
    /// Taxa de decaimento por ciclo (0.0 = sem decaimento, 0.1 = 10% por ciclo)
    pub decay_rate: f64,
    
    /// Energia mínima antes de entrar em Dormant
    pub dormant_threshold: f64,
    
    /// Energia mínima antes de entrar em Fading (candidato a GC)
    pub fading_threshold: f64,
    
    /// Boost de energia ao receber evidência
    pub evidence_boost: f64,
    
    /// Boost de energia ao participar de inferência
    pub inference_boost: f64,
}

/// Sistema de Attention (o "metabolismo" epistêmico)
pub trait AttentionSystem {
    /// Ciclo de decaimento: todos os Concepts perdem energia
    /// Concepts com energia < threshold mudam de estado
    fn decay_cycle(&mut self, domain: &mut Domain);
    
    /// Reforça um Concept (ao receber evidência ou ser consultado)
    fn reinforce(&mut self, concept_id: &ConceptId, amount: f64);
    
    /// Distribui energia limitada entre Concepts concorrentes
    /// (inspirado em NARS budget allocation)
    fn allocate_budget(&mut self, domain: &mut Domain);
    
    /// Coleta de lixo: arquiva Concepts em Fading há muito tempo
    fn garbage_collect(&mut self, domain: &mut Domain) -> Vec<ConceptId>;
}
```

### 7.2 Decaimento Exponencial com Reforço

```rust
/// Função de decaimento de energia
/// energy(t+1) = energy(t) × (1 - decay_rate) + reinforcement
///
/// Analogia: Como uma planta — sem água (evidência), murcha.
/// Com água regular, floresce. Excesso de água não ajuda muito.
fn decay_energy(
    current_energy: f64,
    decay_rate: f64,
    reinforcement: f64,
) -> f64 {
    let decayed = current_energy * (1.0 - decay_rate);
    (decayed + reinforcement).min(1.0).max(0.0)
}
```

---

## 8. INFERENCE — Propagação e Derivação

### 8.1 Motor de Inferência

```rust
/// Sistema de inferência — deriva novos conhecimentos
pub trait InferenceEngine {
    /// Dedução: Se A→B e B→C, então A→C
    /// (com TruthValue calculado pela função de dedução NARS)
    fn deduce(&self, premise1: &Link, premise2: &Link) -> Option<Link>;
    
    /// Indução: Se A→B e A→C, então B→C (com baixa confiança)
    fn induce(&self, premise1: &Link, premise2: &Link) -> Option<Link>;
    
    /// Abdução: Se A→B e C→B, então A→C (hipótese)
    fn abduce(&self, premise1: &Link, premise2: &Link) -> Option<Link>;
    
    /// Propagação: quando um Concept muda, propaga para vizinhos
    /// A intensidade atenua com a distância (TTL)
    fn propagate(&self, changed: &ConceptId, change_type: ChangeType, ttl: u8);
}

pub enum ChangeType {
    EvidenceAdded(Polarity),
    EnergyBoosted,
    StateChanged(ConceptState),
    Archived,
}
```

---

## 9. KNOWLEDGEBASE — Container Raiz

```rust
/// O container raiz — equivale a um "repositório Git de conhecimento"
pub struct KnowledgeBase {
    /// ID único desta réplica
    pub replica_id: ReplicaId,
    
    /// Todos os Domains
    pub domains: HashMap<DomainId, Domain>,
    
    /// Todos os Concepts (indexados por ID)
    pub concepts: HashMap<ConceptId, Concept>,
    
    /// Todos os Links (indexados por ID)
    pub links: HashMap<LinkId, Link>,
    
    /// O Merkle-DAG completo
    pub dag: MerkleDAG,
    
    /// Relógio lógico desta réplica
    pub clock: HybridTimestamp,
    
    /// Configuração global
    pub config: KnowledgeBaseConfig,
}

/// API pública da KnowledgeBase
impl KnowledgeBase {
    // === Criação ===
    pub fn new(replica_id: ReplicaId, config: KnowledgeBaseConfig) -> Self;
    
    // === Concepts ===
    pub fn create_concept(&mut self, label: &str, truth: TruthValue, domain: DomainId) -> ConceptId;
    pub fn get_concept(&self, id: &ConceptId) -> Option<&Concept>;
    pub fn add_evidence(&mut self, concept_id: &ConceptId, evidence: Evidence);
    pub fn query_concepts(&self, predicate: impl Fn(&Concept) -> bool) -> Vec<&Concept>;
    
    // === Links ===
    pub fn create_link(&mut self, kind: LinkKind, participants: Vec<Participant>, truth: TruthValue) -> LinkId;
    pub fn get_link(&self, id: &LinkId) -> Option<&Link>;
    pub fn query_links(&self, concept_id: &ConceptId) -> Vec<&Link>;
    pub fn query_links_by_role(&self, concept_id: &ConceptId, role: &Role) -> Vec<&Link>;
    
    // === Domains ===
    pub fn create_domain(&mut self, name: &str, parent: Option<DomainId>) -> DomainId;
    pub fn get_domain(&self, id: &DomainId) -> Option<&Domain>;
    
    // === Inferência ===
    pub fn infer(&mut self) -> Vec<Link>; // Executa um ciclo de inferência
    
    // === Attention ===
    pub fn attention_cycle(&mut self); // Decaimento + GC
    
    // === Distribuição (Merkle-CRDT) ===
    pub fn heads(&self) -> Vec<MerkleNodeId>;
    pub fn compute_missing(&self, remote_heads: &[MerkleNodeId]) -> Vec<MerkleNodeId>;
    pub fn export_nodes(&self, ids: &[MerkleNodeId]) -> Vec<MerkleNode>;
    pub fn import_nodes(&mut self, nodes: Vec<MerkleNode>) -> Result<SyncReport, SyncError>;
    
    // === Busca Semântica ===
    pub fn search(&self, query: &str) -> Vec<SearchResult>;
    pub fn shortest_path(&self, from: &ConceptId, to: &ConceptId) -> Option<Vec<ConceptId>>;
    pub fn neighbors(&self, id: &ConceptId, depth: usize) -> Vec<&Concept>;
}
```

---

## 10. MÓDULOS RUST — Estrutura do Crate

```
semantic/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # Re-exports públicos
│   │
│   ├── core/                   # Tipos fundamentais
│   │   ├── mod.rs
│   │   ├── concept.rs          # Concept + ConceptState
│   │   ├── link.rs             # Link + Participant + Role + LinkKind
│   │   ├── domain.rs           # Domain + DomainId
│   │   ├── truth_value.rs      # TruthValue + NARS functions
│   │   ├── evidence.rs         # Evidence + EvidenceBag + Polarity
│   │   └── ids.rs              # ContentHash, ConceptId, LinkId, etc.
│   │
│   ├── merkle/                 # Merkle-CRDT
│   │   ├── mod.rs
│   │   ├── node.rs             # MerkleNode + MerklePayload
│   │   ├── dag.rs              # MerkleDAG (storage + traversal)
│   │   ├── sync.rs             # SyncProtocol + anti-entropy
│   │   └── hlc.rs              # HybridTimestamp (Hybrid Logical Clock)
│   │
│   ├── attention/              # Sistema de energia e decaimento
│   │   ├── mod.rs
│   │   ├── config.rs           # AttentionConfig
│   │   ├── decay.rs            # Funções de decaimento
│   │   └── gc.rs               # Garbage collection distribuído
│   │
│   ├── inference/              # Motor de inferência NARS
│   │   ├── mod.rs
│   │   ├── rules.rs            # Dedução, indução, abdução, revisão
│   │   ├── propagation.rs      # Propagação de mudanças
│   │   └── composition.rs      # Composição categórica de Links
│   │
│   ├── query/                  # Busca e consulta
│   │   ├── mod.rs
│   │   ├── semantic_search.rs  # Busca por similaridade
│   │   ├── pattern_match.rs    # Pattern matching em Links
│   │   └── traversal.rs        # BFS/DFS, shortest path
│   │
│   ├── knowledge_base.rs       # KnowledgeBase (container raiz)
│   │
│   └── storage/                # Backend de persistência (trait + impl)
│       ├── mod.rs
│       ├── backend.rs          # StorageBackend trait
│       ├── memory.rs           # In-memory (default)
│       └── sled.rs             # Sled embedded DB (opcional)
│
├── tests/
│   ├── truth_value_tests.rs    # Propriedades CRDT do TruthValue
│   ├── merkle_tests.rs         # Convergência do Merkle-DAG
│   ├── sync_tests.rs           # Sincronização entre réplicas
│   ├── inference_tests.rs      # Regras de inferência NARS
│   ├── nary_link_tests.rs      # Links N-ários com roles
│   └── integration_tests.rs    # Cenários end-to-end
│
├── benches/
│   ├── sync_benchmark.rs       # Performance de sincronização
│   └── query_benchmark.rs      # Performance de consulta
│
└── examples/
    ├── basic_usage.rs           # Criar Concepts, Links, consultar
    ├── distributed_sync.rs      # Duas réplicas sincronizando
    └── medical_ontology.rs      # Exemplo de ontologia médica N-ária
```

---

## 11. DEPENDÊNCIAS RUST

```toml
[package]
name = "semantic"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
blake3 = "1.8"                  # Hash Merkle (BLAKE3)
uuid = { version = "1.11", features = ["v4", "serde"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"               # Error handling
smallvec = "1.13"               # Otimização para Vec pequenos
indexmap = "2.7"                 # HashMap ordered (determinismo)
parking_lot = "0.12"            # RwLock performante
dashmap = "6.1"                 # Concurrent HashMap

[dev-dependencies]
proptest = "1.5"                # Property-based testing
criterion = "0.5"               # Benchmarks
quickcheck = "1.0"              # Mais property testing

[features]
default = ["memory-storage"]
memory-storage = []
sled-storage = ["sled"]
full = ["memory-storage", "sled-storage"]

[dependencies.sled]
version = "0.34"
optional = true
```

---

## 12. INVARIANTES FORMAIS (para testes)

```rust
// === TruthValue CRDT Invariants ===
// INV-TV-1: revision é comutativo
assert_eq!(tv_a.revision(&tv_b), tv_b.revision(&tv_a));

// INV-TV-2: revision é associativo
assert_eq!(tv_a.revision(&tv_b.revision(&tv_c)), tv_a.revision(&tv_b).revision(&tv_c));

// INV-TV-3: revision com dedup é idempotente
assert_eq!(tv_a.revision(&tv_a), tv_a); // com mesmo EvidenceBag

// INV-TV-4: confidence nunca alcança 1.0
assert!(tv.confidence() < 1.0);

// INV-TV-5: frequency ∈ [0.0, 1.0]
assert!((0.0..=1.0).contains(&tv.frequency()));

// === Merkle-DAG Invariants ===
// INV-MK-1: hash determinístico
assert_eq!(node.compute_id(), node.compute_id());

// INV-MK-2: DAG acíclico
assert!(dag.is_acyclic());

// INV-MK-3: todos os parents existem
for parent in &node.parents { assert!(dag.contains(parent)); }

// INV-MK-4: convergência após sync completo
// Se A e B trocam todos os nós, seus estados são idênticos
assert_eq!(kb_a.materialized_state(), kb_b.materialized_state());

// === Link N-ário Invariants ===
// INV-LK-1: todo Link tem ≥ 2 participantes
assert!(link.participants.len() >= 2);

// INV-LK-2: todo participante referencia Concept existente
for p in &link.participants { assert!(kb.concepts.contains_key(&p.concept_id)); }

// INV-LK-3: hash é determinístico (mesmo Link = mesmo hash)
assert_eq!(link_a.id, link_b.id); // se mesmo conteúdo

// === Attention Invariants ===
// INV-AT-1: energia ∈ [0.0, 1.0]
assert!((0.0..=1.0).contains(&concept.energy));

// INV-AT-2: decaimento é monotonicamente decrescente (sem reforço)
// energy(t+1) <= energy(t) quando reinforcement = 0

// INV-AT-3: Archived concepts nunca voltam a Active
// (tombstone para CRDT)

// === HLC Invariants ===
// INV-HLC-1: causalidade preservada
// Se A causou B, então hlc(A) < hlc(B)

// INV-HLC-2: monotonicamente crescente por réplica
// hlc(event_n+1) > hlc(event_n) para mesma réplica
```

---

## 13. FASES DE IMPLEMENTAÇÃO

```
Fase 1: Core Types (1-2 dias)
  → TruthValue, Evidence, EvidenceBag, ConceptId, LinkId
  → Testes de propriedades CRDT do TruthValue
  
Fase 2: Concept + Link + Domain (2-3 dias)
  → Structs completos com serialização
  → Links N-ários com roles
  → Testes de criação e consulta
  
Fase 3: Merkle-DAG (3-4 dias)
  → MerkleNode, hash BLAKE3, DAG storage
  → Inserção e traversal do DAG
  → Testes de integridade (hash determinístico, acíclico)
  
Fase 4: Sincronização (3-4 dias)
  → HLC (Hybrid Logical Clock)
  → Protocolo anti-entropy
  → Sync entre 2 réplicas
  → Testes de convergência
  
Fase 5: KnowledgeBase + API (2-3 dias)
  → Container raiz com operações CRUD
  → Índices para consulta eficiente
  → API pública documentada
  
Fase 6: Inference Engine (2-3 dias)
  → Regras NARS (dedução, indução, abdução)
  → Propagação de mudanças
  → Composição categórica
  
Fase 7: Attention System (1-2 dias)
  → Decaimento, reforço, GC
  → Distribuição de budget
  
Fase 8: Query + Storage (2-3 dias)
  → Busca semântica, pattern matching, traversal
  → Storage backend trait + implementação in-memory
  → Exemplos e documentação final
```

---

## 14. REFERÊNCIAS CIENTÍFICAS

| # | Referência | Contribuição |
|---|---|---|
| 1 | Sanjuán, Pöyhtäri, Teixeira (2020). "Merkle-CRDTs: Merkle-DAGs meet CRDTs" | Fundação teórica do Merkle-CRDT |
| 2 | Shapiro et al. (2011). "Conflict-Free Replicated Data Types" | CRDTs state-based e op-based |
| 3 | Almeida et al. (2018). "Delta State Replicated Data Types" | δ-CRDTs para eficiência |
| 4 | Wang, Pei (2006). "Rigid Flexibility: The Logic of Intelligence" | NARS, NAL, TruthValue |
| 5 | Wang, Pei (2013). "Non-Axiomatic Logic: A Model of Intelligent Reasoning" | Regras de inferência formais |
| 6 | Kulkarni et al. (2014). "Logical Physical Clocks and Consistent Snapshots" | Hybrid Logical Clocks |
| 7 | Fatemi et al. (2020). "Knowledge Hypergraphs: Prediction Beyond Binary Relations" | Hyperlinks N-ários |
| 8 | Luo et al. (2025). "HyperGraphRAG: Hypergraph-Structured Knowledge Representation" | Aplicação de hypergraphs em knowledge bases |
| 9 | Maturana & Varela (1980). "Autopoiesis and Cognition" | Autopoiese como inspiração para auto-reparo |
| 10 | Baader et al. (2003). "The Description Logic Handbook" | Ontologias formais |
| 11 | Awodey (2010). "Category Theory" | Composição de morfismos |
| 12 | IPFS go-ds-crdt implementation | Merkle-CRDT em produção (100M keys) |
