// 引擎算法层：被 services/compare_service 编排。
// 解析(parse) → 标准化(normalize) → 分块(chunker) → 特征(features/corpus) →
// 召回(candidate) → 评分(scoring) → 聚类(clustering) → 分类与 diff(diff) →
// 矩阵(matrix) + 事实冲突(fact) + 围标判定(collusion) + 指纹(fingerprint)。
pub mod candidate;
pub mod chunker;
pub mod clustering;
pub mod collusion;
pub mod corpus;
pub mod diff;
pub mod embed;
pub mod fact;
pub mod features;
pub mod fingerprint;
pub mod matrix;
pub mod normalize;
pub mod ocr;
pub mod parse;
pub mod report;
pub mod scoring;
pub mod segment;
pub mod similarity;
