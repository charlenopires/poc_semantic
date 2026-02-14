# Calibracao nomic-embed-text — Thresholds e Normalizacao

## Modelo

- **Modelo**: `nomic-embed-text` via LM Studio (OpenAI-compatible API)
- **Dimensionalidade**: 768
- **Prefixos**: `search_document:` para indexacao, `search_query:` para consultas

## Thresholds calibrados

| Parametro | Valor | Uso |
|-----------|-------|-----|
| Merge de conceitos | 0.90 | Similaridade coseno minima para considerar dois conceitos como o mesmo |
| Auto-similaridade | 0.78–0.90 | Faixa tipica entre variantes do mesmo conceito (singular/plural, com/sem acento) |
| Query (busca) | 0.35 | Similaridade minima para retornar um conceito como relevante para uma query |

### Notas sobre os thresholds

- **0.90 (merge)**: Conservador para evitar juntar conceitos distintos. Exemplos de pares que atingem >= 0.90: "sustentabilidade" / "sustentavel", "operacional" / "operacao".
- **0.78–0.90 (auto-similaridade)**: Variantes morfologicas do mesmo conceito tipicamente caem nessa faixa. Pares abaixo de 0.78 geralmente sao conceitos relacionados mas distintos.
- **0.35 (query)**: Limiar baixo intencional — preferimos recall sobre precision na busca, ja que o ranking por similaridade ordena os resultados.

## Normalizacao de palavras quebradas

O `pdf_extract` frequentemente quebra palavras portuguesas em posicoes arbitrarias ao extrair texto de PDFs. Isso gera fragmentos sem sentido que poluem a KB.

### Camada 1: Regex de sufixos (`pdf.rs`)

Primeira linha de defesa. Junta palavras que terminam separadas de seus sufixos comuns:

```
(\w+)\s+(cao|coes|cia|encia|ancia|mente|dade|avel|ivel|nal|gem|tico|tica|tura|mento|sao|soes|oso|osa|ivo|iva|ismo|ista)
```

Exemplos:
- `opera cao` → `operacao`
- `sustenta bilidade` → N/A (coberto por `dade`)
- `operacio nal` → `operacional`

### Camada 2: Heuristica de fragmentos (`extractor.rs`)

Segunda linha de defesa, mais generica. Detecta tokens curtos (2-4 chars) que nao sao palavras reais conhecidas e os junta com tokens adjacentes.

**Criterios de fragmento**:
- 2-4 caracteres
- Nao e stopword PT-BR
- Nao e palavra curta conhecida (lista curada: sol, caso, base, etc.)
- Nao parece verbo (sufixo verbal)
- Composto apenas por letras

**Regras de acumulacao**:
- Ao detectar fragmento, acumula tokens seguintes que iniciam com minuscula
- Para quando: acumulado >= 6 chars E proximo nao e fragmento
- Para se proximo e stopword ou palavra capitalizada real

Exemplos:
- `arm azenagem` → `armazenagem`
- `Oper acio nal` → `Operacional`
- `caso alto base` → `caso alto base` (preservado — palavras reais)

### Safety net: filtro de n-grams

Alem da normalizacao, o filtro de n-grams no extrator exige minimo de 5 chars por palavra componente. Isso impede que fragmentos residuais (que escaparam das duas camadas anteriores) formem entidades compostas sem sentido.

## Resultados empiricos

Antes da normalizacao, PDFs em PT-BR geravam entidades como:
- `arm`, `acio`, `nal`, `Oper`, `gem`, `tico`

Apos as duas camadas de normalizacao + filtro de n-grams, esses fragmentos sao eliminados ou reunificados em palavras completas.
