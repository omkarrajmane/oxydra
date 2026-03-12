# Migration Plan: FastAPI → Supabase (Big Bang Strategy)

## Executive Summary
- **Strategy**: Big Bang (1-2 days downtime)
- **Backend Reduction**: ~70% code removal (8,000 → ~2,000 lines)
- **Remaining Backend**: Chat/RAG + Pipeline V3 only
- **Database**: Full migration to Supabase Postgres with pgvector

## Current Architecture

### Backend (To Be Reduced)
**Current FastAPI Endpoints:**
- **Auth**: Login, Register, Webhook (will be removed)
- **Read-Only**: Podcasts, Episodes, Ideas, Quotes, Insights, Products (move to Supabase)
- **User Writes**: Votes, Saved Ideas (move to Supabase)
- **Chat/RAG**: Chat endpoints, Indexing (keep in backend)
- **Pipeline**: Pipeline V3 trigger/status (keep in backend)

**Pipeline V3:**
- Currently writes to DB via SQLAlchemy in `pipeline_v3/services/db_client.py`
- Will update to write directly to Supabase Postgres

### Frontend
**Currently calls:**
- Supabase Auth (already using)
- FastAPI endpoints via axios (`src/services/api.ts`)

**Will change to:**
- Supabase Auth (no change)
- Supabase client for data queries (major refactor)
- FastAPI only for chat and pipeline (minimal calls)

### Database
**Tables to migrate to Supabase:**
1. podcasts
2. episodes
3. ideas
4. business_plans
5. quotes
6. insights
7. products
8. topics
9. tags
10. votes (with RLS)
11. saved_ideas (with RLS)
12. transcript_chunks (pgvector)
13. subscribers

**Tables to remove/keep in backend:**
- alembic_version (not needed)
- chat_sessions (keep in backend SQLite or separate)
- chat_messages (keep in backend)
- users (use Supabase Auth instead)

---

## Phase 1: Database Migration (Day 1 - Morning)

### 1.1 Create Supabase Schema

Run this SQL in Supabase SQL Editor:

```sql
-- ============================================================================
-- SUPABASE MIGRATION DDL
-- ============================================================================

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgvector";

-- ============================================================================
-- CORE TABLES (Public read, user-specific writes)
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.podcasts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    channel_url TEXT,
    title TEXT,
    description TEXT,
    channel_name TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_by UUID REFERENCES auth.users(id),
    episode_metadata JSONB DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS public.episodes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    podcast_id UUID REFERENCES public.podcasts(id) ON DELETE CASCADE,
    title TEXT,
    description TEXT,
    guest_name TEXT,
    youtube_url TEXT,
    thumbnail_url TEXT,
    transcript TEXT,
    is_private BOOLEAN DEFAULT FALSE,
    notes TEXT,
    summary TEXT,
    episode_metadata JSONB DEFAULT '{}',
    published_date TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.ideas (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    episode_id UUID REFERENCES public.episodes(id) ON DELETE CASCADE,
    title TEXT,
    short_description TEXT,
    detailed_description TEXT,
    business_area TEXT,
    market_size NUMERIC,
    normalized_market_size_usd NUMERIC,
    timeline TEXT,
    currency TEXT,
    upvotes INTEGER DEFAULT 0,
    downvotes INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_by UUID REFERENCES auth.users(id)
);

CREATE TABLE IF NOT EXISTS public.business_plans (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    idea_id UUID REFERENCES public.ideas(id) ON DELETE CASCADE UNIQUE,
    name TEXT,
    short_description TEXT,
    detailed_description TEXT,
    value_propositions JSONB DEFAULT '[]',
    challenges JSONB DEFAULT '[]',
    opportunities JSONB DEFAULT '[]',
    target_market JSONB DEFAULT '[]',
    revenue_model JSONB DEFAULT '[]',
    foundation_setup JSONB DEFAULT '{}',
    program_development JSONB DEFAULT '{}',
    market_size TEXT,
    timeline TEXT,
    initial_investment TEXT,
    currency TEXT,
    team_size TEXT,
    competition_level TEXT,
    market_trend TEXT,
    relevant_quotes JSONB DEFAULT '[]',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.quotes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    episode_id UUID REFERENCES public.episodes(id) ON DELETE CASCADE,
    text TEXT,
    speaker TEXT,
    context TEXT,
    timestamp TEXT,
    timestamp_seconds INTEGER,
    topics JSONB DEFAULT '[]',
    ai_quality_score INTEGER,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.insights (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    episode_id UUID REFERENCES public.episodes(id) ON DELETE CASCADE,
    title TEXT,
    description TEXT,
    related_topics JSONB DEFAULT '[]',
    supporting_quotes JSONB DEFAULT '[]',
    timestamp_seconds INTEGER,
    ai_quality_score INTEGER,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    episode_id UUID REFERENCES public.episodes(id) ON DELETE CASCADE,
    name TEXT,
    description TEXT,
    url TEXT,
    price NUMERIC,
    category TEXT,
    mentioned_at TEXT,
    is_sponsored BOOLEAN DEFAULT FALSE,
    rating NUMERIC,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.topics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    episode_id UUID REFERENCES public.episodes(id) ON DELETE CASCADE,
    name TEXT,
    description TEXT,
    relevance_score NUMERIC,
    subtopics JSONB DEFAULT '[]',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.tags (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT UNIQUE,
    category TEXT
);

-- ============================================================================
-- USER-SPECIFIC TABLES (RLS Protected)
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.votes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    idea_id UUID REFERENCES public.ideas(id) ON DELETE CASCADE,
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,
    is_upvote BOOLEAN NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(idea_id, user_id)
);

CREATE TABLE IF NOT EXISTS public.saved_ideas (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,
    idea_id UUID REFERENCES public.ideas(id) ON DELETE CASCADE,
    saved_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(user_id, idea_id)
);

CREATE TABLE IF NOT EXISTS public.subscribers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email TEXT UNIQUE NOT NULL,
    source TEXT DEFAULT 'footer',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- ============================================================================
-- VECTOR STORAGE (For RAG/Chat)
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.transcript_chunks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    episode_id UUID REFERENCES public.episodes(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    embedding VECTOR(1536),
    start_time INTEGER,
    end_time INTEGER,
    speaker TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(episode_id, chunk_index)
);

CREATE INDEX ON public.transcript_chunks 
USING ivfflat (embedding vector_cosine_ops)
WITH (lists = 100);

-- ============================================================================
-- INDEXES
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_podcasts_channel_url ON public.podcasts(channel_url);
CREATE INDEX IF NOT EXISTS idx_episodes_podcast_id ON public.episodes(podcast_id);
CREATE INDEX IF NOT EXISTS idx_episodes_published_date ON public.episodes(published_date DESC);
CREATE INDEX IF NOT EXISTS idx_ideas_episode_id ON public.ideas(episode_id);
CREATE INDEX IF NOT EXISTS idx_ideas_business_area ON public.ideas(business_area);
CREATE INDEX IF NOT EXISTS idx_ideas_upvotes ON public.ideas(upvotes DESC);
CREATE INDEX IF NOT EXISTS idx_quotes_episode_id ON public.quotes(episode_id);
CREATE INDEX IF NOT EXISTS idx_insights_episode_id ON public.insights(episode_id);
CREATE INDEX IF NOT EXISTS idx_transcript_chunks_episode_id ON public.transcript_chunks(episode_id);
CREATE INDEX IF NOT EXISTS idx_votes_user_id ON public.votes(user_id);
CREATE INDEX IF NOT EXISTS idx_saved_ideas_user_id ON public.saved_ideas(user_id);

-- ============================================================================
-- RLS POLICIES
-- ============================================================================

ALTER TABLE public.podcasts ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.episodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.ideas ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.business_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.quotes ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.insights ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.products ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.topics ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.tags ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.transcript_chunks ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.votes ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.saved_ideas ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.subscribers ENABLE ROW LEVEL SECURITY;

-- Public read access
CREATE POLICY "Podcasts are publicly readable" ON public.podcasts FOR SELECT USING (true);
CREATE POLICY "Episodes are publicly readable" ON public.episodes FOR SELECT USING (true);
CREATE POLICY "Ideas are publicly readable" ON public.ideas FOR SELECT USING (true);
CREATE POLICY "Business plans are publicly readable" ON public.business_plans FOR SELECT USING (true);
CREATE POLICY "Quotes are publicly readable" ON public.quotes FOR SELECT USING (true);
CREATE POLICY "Insights are publicly readable" ON public.insights FOR SELECT USING (true);
CREATE POLICY "Products are publicly readable" ON public.products FOR SELECT USING (true);
CREATE POLICY "Topics are publicly readable" ON public.topics FOR SELECT USING (true);
CREATE POLICY "Tags are publicly readable" ON public.tags FOR SELECT USING (true);
CREATE POLICY "Transcript chunks are publicly readable" ON public.transcript_chunks FOR SELECT USING (true);

-- User-specific write access
CREATE POLICY "Users can view their own votes" ON public.votes FOR SELECT USING (auth.uid() = user_id);
CREATE POLICY "Users can insert their own votes" ON public.votes FOR INSERT WITH CHECK (auth.uid() = user_id);
CREATE POLICY "Users can update their own votes" ON public.votes FOR UPDATE USING (auth.uid() = user_id);
CREATE POLICY "Users can delete their own votes" ON public.votes FOR DELETE USING (auth.uid() = user_id);

CREATE POLICY "Users can view their own saved ideas" ON public.saved_ideas FOR SELECT USING (auth.uid() = user_id);
CREATE POLICY "Users can insert their own saved ideas" ON public.saved_ideas FOR INSERT WITH CHECK (auth.uid() = user_id);
CREATE POLICY "Users can delete their own saved ideas" ON public.saved_ideas FOR DELETE USING (auth.uid() = user_id);

CREATE POLICY "Anyone can subscribe to newsletter" ON public.subscribers FOR INSERT WITH CHECK (true);

-- ============================================================================
-- FUNCTIONS & TRIGGERS
-- ============================================================================

CREATE OR REPLACE FUNCTION public.update_idea_vote_counts()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.is_upvote THEN
            UPDATE public.ideas SET upvotes = upvotes + 1 WHERE id = NEW.idea_id;
        ELSE
            UPDATE public.ideas SET downvotes = downvotes + 1 WHERE id = NEW.idea_id;
        END IF;
        RETURN NEW;
    ELSIF TG_OP = 'DELETE' THEN
        IF OLD.is_upvote THEN
            UPDATE public.ideas SET upvotes = upvotes - 1 WHERE id = OLD.idea_id;
        ELSE
            UPDATE public.ideas SET downvotes = downvotes - 1 WHERE id = OLD.idea_id;
        END IF;
        RETURN OLD;
    ELSIF TG_OP = 'UPDATE' THEN
        IF OLD.is_upvote != NEW.is_upvote THEN
            IF NEW.is_upvote THEN
                UPDATE public.ideas SET upvotes = upvotes + 1, downvotes = downvotes - 1 WHERE id = NEW.idea_id;
            ELSE
                UPDATE public.ideas SET upvotes = upvotes - 1, downvotes = downvotes + 1 WHERE id = NEW.idea_id;
            END IF;
        END IF;
        RETURN NEW;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE TRIGGER update_idea_votes
AFTER INSERT OR UPDATE OR DELETE ON public.votes
FOR EACH ROW EXECUTE FUNCTION public.update_idea_vote_counts();

-- Vector similarity search function for RAG
CREATE OR REPLACE FUNCTION public.match_transcript_chunks(
    query_embedding VECTOR(1536),
    match_threshold FLOAT,
    match_count INT,
    episode_filter UUID DEFAULT NULL
)
RETURNS TABLE (
    id UUID,
    episode_id UUID,
    chunk_index INTEGER,
    content TEXT,
    similarity FLOAT,
    start_time INTEGER,
    end_time INTEGER,
    speaker TEXT
) AS $$
BEGIN
    IF episode_filter IS NOT NULL THEN
        RETURN QUERY
        SELECT tc.id, tc.episode_id, tc.chunk_index, tc.content,
               1 - (tc.embedding <=> query_embedding) AS similarity,
               tc.start_time, tc.end_time, tc.speaker
        FROM public.transcript_chunks tc
        WHERE tc.episode_id = episode_filter
        AND 1 - (tc.embedding <=> query_embedding) > match_threshold
        ORDER BY tc.embedding <=> query_embedding
        LIMIT match_count;
    ELSE
        RETURN QUERY
        SELECT tc.id, tc.episode_id, tc.chunk_index, tc.content,
               1 - (tc.embedding <=> query_embedding) AS similarity,
               tc.start_time, tc.end_time, tc.speaker
        FROM public.transcript_chunks tc
        WHERE 1 - (tc.embedding <=> query_embedding) > match_threshold
        ORDER BY tc.embedding <=> query_embedding
        LIMIT match_count;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Grant permissions
GRANT USAGE ON SCHEMA public TO authenticated, anon;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO authenticated;
GRANT SELECT ON public.podcasts, public.episodes, public.ideas, public.business_plans, 
    public.quotes, public.insights, public.products, public.topics, public.tags, 
    public.transcript_chunks TO anon;
GRANT INSERT ON public.subscribers TO anon;
GRANT EXECUTE ON FUNCTION public.match_transcript_chunks TO authenticated, anon;
```

### 1.2 Data Migration Script

Create a Python script to migrate data from old DB to Supabase:

```python
# scripts/migrate_data_to_supabase.py
import asyncio
import os
from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlalchemy.orm import sessionmaker
from supabase import create_client, Client
import json
from datetime import datetime

# Old database connection
OLD_DB_URL = os.getenv("DATABASE_URL")  # Your current Postgres URL

# Supabase connection
SUPABASE_URL = os.getenv("SUPABASE_URL")
SUPABASE_KEY = os.getenv("SUPABASE_SERVICE_KEY")  # Service role key for migration

# Create engine for old DB
old_engine = create_async_engine(OLD_DB_URL)
OldSession = sessionmaker(old_engine, class_=AsyncSession)

# Create Supabase client
supabase: Client = create_client(SUPABASE_URL, SUPABASE_KEY)

async def migrate_table(table_name: str, batch_size: int = 1000):
    """Generic table migration with batching"""
    async with OldSession() as session:
        # Get all records from old table
        result = await session.execute(f"SELECT * FROM {table_name}")
        records = result.mappings().all()
        
        print(f"Migrating {len(records)} records from {table_name}...")
        
        # Convert to dict and handle datetime/UUID serialization
        batch = []
        for i, record in enumerate(records):
            row = dict(record)
            # Convert datetime objects to ISO strings
            for key, value in row.items():
                if isinstance(value, datetime):
                    row[key] = value.isoformat()
                elif isinstance(value, dict):
                    row[key] = json.dumps(value)
            
            batch.append(row)
            
            if len(batch) >= batch_size:
                # Insert batch to Supabase
                supabase.table(table_name).insert(batch).execute()
                print(f"  Inserted {i+1}/{len(records)} records...")
                batch = []
        
        # Insert remaining records
        if batch:
            supabase.table(table_name).insert(batch).execute()
        
        print(f"✓ Migrated {len(records)} records to {table_name}")

async def main():
    tables = [
        "podcasts",
        "episodes", 
        "ideas",
        "business_plans",
        "quotes",
        "insights",
        "products",
        "topics",
        "tags",
        "votes",
        "saved_ideas",
        "subscribers",
        # Note: transcript_chunks migrated separately due to size
    ]
    
    for table in tables:
        try:
            await migrate_table(table)
        except Exception as e:
            print(f"✗ Error migrating {table}: {e}")
            raise

if __name__ == "__main__":
    asyncio.run(main())
```

---

## Phase 2: Frontend Refactoring (Day 1 - Afternoon)

### 2.1 Create Supabase Service Layer

Create `src/services/supabase.ts`:

```typescript
import { createClient } from '@supabase/supabase-js';
import type { Podcast, Episode, Idea, Insight, Quote, Product } from '../types';

const supabaseUrl = import.meta.env.VITE_SUPABASE_URL;
const supabaseKey = import.meta.env.VITE_SUPABASE_ANON_KEY;

export const supabaseClient = createClient(supabaseUrl, supabaseKey);

// ============================================================================
// READ OPERATIONS (Previously: GET /podcasts/, GET /podcasts/:id)
// ============================================================================

export async function getPodcasts(): Promise<Podcast[]> {
  const { data, error } = await supabaseClient
    .from('podcasts')
 .select('*')
    .order('created_at', { ascending: false });
  
  if (error) throw error;
  return data || [];
}

export async function getPodcast(id: string): Promise<Podcast> {
  const { data, error } = await supabaseClient
    .from('podcasts')
    .select('*')
    .eq('id', id)
    .single();
  
  if (error) throw error;
  return data;
}

// ============================================================================
// EPISODES (Previously: GET /episodes/:podcastId)
// ============================================================================

export async function getEpisodes(podcastId: string): Promise<Episode[]> {
  const { data, error } = await supabaseClient
    .from('episodes')
    .select('*')
    .eq('podcast_id', podcastId)
    .order('published_date', { ascending: false });
  
  if (error) throw error;
  return data || [];
}

export async function getEpisodeDetails(episodeId: string): Promise<Episode> {
  const { data, error } = await supabaseClient
    .from('episodes')
    .select('*')
    .eq('id', episodeId)
    .single();
  
  if (error) throw error;
  return data;
}

// ============================================================================
// IDEAS (Previously: GET /ideas/episode/:episodeId, GET /ideas/)
// ============================================================================

export async function getIdeas(episodeId: string): Promise<Idea[]> {
  const { data, error } = await supabaseClient
    .from('ideas')
    .select(`
      *,
      votes:votes(is_upvote, user_id)
    `)
    .eq('episode_id', episodeId)
    .order('upvotes', { ascending: false });
  
  if (error) throw error;
  return data || [];
}

interface GetAllIdeasParams {
  page?: number;
  pageSize?: number;
  sortBy?: 'business_area' | 'market_size' | 'upvotes';
  sortOrder?: 'asc' | 'desc';
  businessArea?: string;
  keyword?: string;
}

export async function getAllIdeas(params: GetAllIdeasParams = {}) {
  const { page = 1, pageSize = 10, sortBy, sortOrder, businessArea, keyword } = params;
  
  let query = supabaseClient
    .from('ideas')
    .select(`
      *,
      episode:episodes(*, podcast:podcasts(*)),
      votes:votes(is_upvote)
    `, { count: 'exact' });
  
  // Apply filters
  if (businessArea) {
    query = query.eq('business_area', businessArea);
  }
  
  if (keyword) {
    query = query.or(`title.ilike.%${keyword}%,short_description.ilike.%${keyword}%`);
  }
  
  // Apply sorting
  if (sortBy) {
    const ascending = sortOrder === 'asc';
    query = query.order(sortBy, { ascending });
  } else {
    query = query.order('created_at', { ascending: false });
  }
  
  // Apply pagination
  const from = (page - 1) * pageSize;
  const to = from + pageSize - 1;
  query = query.range(from, to);
  
  const { data, error, count } = await query;
  
  if (error) throw error;
  
  return {
    items: data || [],
    total: count || 0,
    page,
    pageSize,
    totalPages: Math.ceil((count || 0) / pageSize)
  };
}

export async function getBusinessAreas(): Promise<string[]> {
  const { data, error } = await supabaseClient
    .from('ideas')
    .select('business_area')
    .not('business_area', 'is', null)
    .order('business_area');
  
  if (error) throw error;
  
  // Extract unique business areas
  const areas = [...new Set(data?.map(d => d.business_area))];
  return areas;
}

// ============================================================================
// USER-SPECIFIC OPERATIONS (With RLS)
// ============================================================================

export async function updateIdeaVote(ideaId: string, type: 'upvote' | 'downvote'): Promise<void> {
  const { data: { user } } = await supabaseClient.auth.getUser();
  if (!user) throw new Error('User must be authenticated');
  
  const isUpvote = type === 'upvote';
  
  // Check if user already voted
  const { data: existingVote } = await supabaseClient
    .from('votes')
    .select('id, is_upvote')
    .eq('idea_id', ideaId)
    .eq('user_id', user.id)
    .single();
  
  if (existingVote) {
    if (existingVote.is_upvote === isUpvote) {
      // Remove vote if same type (toggle off)
      await supabaseClient
        .from('votes')
        .delete()
        .eq('id', existingVote.id);
    } else {
      // Update vote type
      await supabaseClient
        .from('votes')
        .update({ is_upvote: isUpvote })
        .eq('id', existingVote.id);
    }
  } else {
    // Insert new vote
    await supabaseClient
      .from('votes')
      .insert({
        idea_id: ideaId,
        user_id: user.id,
        is_upvote: isUpvote
      });
  }
}

export async function saveIdea(ideaId: string): Promise<void> {
  const { data: { user } } = await supabaseClient.auth.getUser();
  if (!user) throw new Error('User must be authenticated');
  
  const { error } = await supabaseClient
    .from('saved_ideas')
    .insert({
      idea_id: ideaId,
      user_id: user.id
    });
  
  if (error) throw error;
}

export async function unsaveIdea(ideaId: string): Promise<void> {
  const { data: { user } } = await supabaseClient.auth.getUser();
  if (!user) throw new Error('User must be authenticated');
  
  const { error } = await supabaseClient
    .from('saved_ideas')
    .delete()
    .eq('idea_id', ideaId)
    .eq('user_id', user.id);
  
  if (error) throw error;
}

export async function getSavedIdeas(page: number = 1, pageSize: number = 10) {
  const { data: { user } } = await supabaseClient.auth.getUser();
  if (!user) throw new Error('User must be authenticated');
  
  const from = (page - 1) * pageSize;
  const to = from + pageSize - 1;
  
  const { data, error, count } = await supabaseClient
    .from('saved_ideas')
    .select(`
      *,
      idea:ideas(*, episode:episodes(*, podcast:podcasts(*)))
    `, { count: 'exact' })
    .eq('user_id', user.id)
    .order('saved_at', { ascending: false })
    .range(from, to);
  
  if (error) throw error;
  
  return {
    items: data?.map(d => d.idea) || [],
    total: count || 0,
    page,
    pageSize,
    totalPages: Math.ceil((count || 0) / pageSize)
  };
}

// ============================================================================
// INSIGHTS, QUOTES, PRODUCTS (Previously: GET endpoints)
// ============================================================================

export async function getInsights(
  episodeId: string, 
  params: { min_ai_quality_score?: number } = {}
): Promise<Insight[]> {
  let query = supabaseClient
    .from('insights')
    .select('*')
    .eq('episode_id', episodeId);
  
  if (params.min_ai_quality_score) {
    query = query.gte('ai_quality_score', params.min_ai_quality_score);
  }
  
  const { data, error } = await query.order('ai_quality_score', { ascending: false });
  
  if (error) throw error;
  return data || [];
}

export async function getQuotes(
  episodeId: string,
  params: { min_ai_quality_score?: number } = {}
): Promise<Quote[]> {
  let query = supabaseClient
    .from('quotes')
    .select('*')
    .eq('episode_id', episodeId);
  
  if (params.min_ai_quality_score) {
    query = query.gte('ai_quality_score', params.min_ai_quality_score);
  }
  
  const { data, error } = await query;
  
  if (error) throw error;
  return data || [];
}

export async function getProducts(episodeId: string): Promise<Product[]> {
  const { data, error } = await supabaseClient
    .from('products')
    .select('*')
    .eq('episode_id', episodeId);
  
  if (error) throw error;
  return data || [];
}

// ============================================================================
// NEWSLETTER SUBSCRIPTION
// ============================================================================

export async function subscribeToNewsletter(email: string, source: string = 'footer'): Promise<void> {
  const { error } = await supabaseClient
    .from('subscribers')
    .insert({ email, source });
  
  if (error) {
    if (error.code === '23505') { // Unique violation
      throw new Error('You are already subscribed!');
    }
    throw error;
  }
}
```

### 2.2 Update API Service

Update `src/services/api.ts` to only include chat and pipeline endpoints:

```typescript
import axios, { AxiosError } from 'axios';
import { supabase } from '../lib/supabase';
import type {
  SendMessageRequest,
  SendMessageResponse,
  SessionsResponse,
  SessionDetailResponse,
  IndexStatus,
  ChatSession,
} from '../types/chat';

const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:8000';
const secureApiUrl = import.meta.env.PROD ? API_URL.replace('http://', 'https://') : API_URL;

// Only for chat and pipeline operations
export const api = axios.create({
  baseURL: `${secureApiUrl}/api/v1`,
  headers: { 'Content-Type': 'application/json' },
  withCredentials: true,
});

// Add auth interceptor
api.interceptors.request.use(
  async (config) => {
    const { data: { session } } = await supabase.auth.getSession();
    if (session?.access_token) {
      config.headers.Authorization = `Bearer ${session.access_token}`;
    }
    return config;
  },
  (error) => Promise.reject(error)
);

// ============================================================================
// CHAT API (Still uses backend - these stay)
// ============================================================================

export const chatAPI = {
  sendMessage: async (episodeId: string, data: SendMessageRequest): Promise<SendMessageResponse> => {
    const response = await api.post<SendMessageResponse>(`/chat/episodes/${episodeId}/chat`, data);
    return response.data;
  },

  listSessions: async (): Promise<SessionsResponse> => {
    const response = await api.get<SessionsResponse>('/chat/sessions');
    return response.data;
  },

  getSession: async (sessionId: string): Promise<SessionDetailResponse> => {
    const response = await api.get<SessionDetailResponse>(`/chat/sessions/${sessionId}`);
    return response.data;
  },

  deleteSession: async (sessionId: string): Promise<void> => {
    await api.delete(`/chat/sessions/${sessionId}`);
  },

  getIndexStatus: async (episodeId: string): Promise<IndexStatus> => {
    const response = await api.get<IndexStatus>(`/chat/episodes/${episodeId}/index-status`);
    return response.data;
  },

  triggerIndexing: async (episodeId: string): Promise<void> => {
    await api.post(`/chat/episodes/${episodeId}/index-transcript`);
  },

  // Podcast-level chat
  sendPodcastMessage: async (podcastId: string, data: SendMessageRequest): Promise<SendMessageResponse> => {
    const response = await api.post<SendMessageResponse>(`/chat/podcasts/${podcastId}/chat`, data);
    return response.data;
  },
};

// ============================================================================
// PIPELINE API (Still uses backend)
// ============================================================================

export const pipelineAPI = {
  triggerProcessing: async (youtubeUrl: string): Promise<{ taskId: string }> => {
    const response = await api.post('/pipeline/trigger', { url: youtubeUrl });
    return response.data;
  },

  getStatus: async (taskId: string): Promise<{
    status: 'pending' | 'processing' | 'completed' | 'failed';
    progress: number;
    message: string;
  }> => {
    const response = await api.get(`/pipeline/status/${taskId}`);
    return response.data;
  },
};
```

### 2.3 Update Components

Example: Update `PodcastsPage.tsx`:

```typescript
// Before (using old API)
import { getPodcasts } from '../services/api';

// After (using Supabase)
import { getPodcasts } from '../services/supabase';
```

---

## Phase 3: Backend Slimming (Day 2 - Morning)

### 3.1 New Slim Backend Structure

```
backend/
├── main.py                    # Only chat + pipeline endpoints
├── app/
│   ├── api/
│   │   └── v1/
│   │       └── endpoints/
│   │           ├── chat.py    # Chat endpoints (keep)
│   │           └── pipeline.py # Pipeline trigger/status (keep)
│   ├── services/
│   │   ├── chat/             # Chat orchestration (keep)
│   │   ├── retrieval/        # Vector retrieval (keep)
│   │   └── embeddings.py     # Embedding service (keep)
│   └── core/
│       └── config.py         # Config (remove old DB settings)
└── pipeline_v3/              # Keep as-is, update DB connection
    └── services/
        └── db_client.py      # Update to use Supabase
```

### 3.2 Remove These Endpoints

Delete or comment out in `main.py`:

```python
# REMOVE these routers:
# app.include_router(auth.router, ...)          # Auth handled by Supabase
# app.include_router(podcasts.router, ...)      # Now in Supabase
# app.include_router(episodes.router, ...)      # Now in Supabase
# app.include_router(ideas.router, ...)         # Now in Supabase
# app.include_router(products.router, ...)      # Now in Supabase
# app.include_router(quotes.router, ...)        # Now in Supabase
# app.include_router(insights.router, ...)      # Now in Supabase
# app.include_router(subscribers.router, ...)   # Now in Supabase
# app.include_router(user.router, ...)          # Webhook no longer needed

# KEEP only these:
app.include_router(chat.router, prefix=f"{settings.API_V1_STR}/chat", tags=["chat"])
app.include_router(pipeline.router, prefix=f"{settings.API_V1_STR}/pipeline", tags=["pipeline"])
```

### 3.3 Create New Pipeline Endpoint

Create `app/api/v1/endpoints/pipeline.py`:

```python
from fastapi import APIRouter, Depends, HTTPException, BackgroundTasks
from app.api import deps
from typing import Optional
import uuid
from datetime import datetime
import asyncio

router = APIRouter()

# In-memory store for task status (use Redis in production)
task_store = {}

@router.post("/trigger")
async def trigger_pipeline(
    url: str,
    background_tasks: BackgroundTasks,
    current_user: str = Depends(deps.get_current_user)
):
    """Trigger Pipeline V3 to process a YouTube URL"""
    task_id = str(uuid.uuid4())
    
    # Initialize task status
    task_store[task_id] = {
        "id": task_id,
        "status": "pending",
        "progress": 0,
        "message": "Task queued",
        "created_at": datetime.utcnow().isoformat(),
        "user_id": current_user,
        "url": url
    }
    
    # Start processing in background
    background_tasks.add_task(process_video, task_id, url)
    
    return {"task_id": task_id, "status": "pending"}

@router.get("/status/{task_id}")
async def get_pipeline_status(
    task_id: str,
    current_user: str = Depends(deps.get_current_user)
):
    """Get processing status for a task"""
    task = task_store.get(task_id)
    
    if not task:
        raise HTTPException(status_code=404, detail="Task not found")
    
    if task["user_id"] != current_user:
        raise HTTPException(status_code=403, detail="Not authorized to view this task")
    
    return task

async def process_video(task_id: str, url: str):
    """Background task to process video through Pipeline V3"""
    try:
        from backend.pipeline_v3.main import PipelineOrchestrator
        
        # Update status
        task_store[task_id]["status"] = "processing"
        task_store[task_id]["message"] = "Starting pipeline"
        
        # Run pipeline
        orchestrator = PipelineOrchestrator(max_concurrent=1)
        
        # Create async task
        loop = asyncio.get_event_loop()
        result = await loop.run_in_executor(
            None, 
            lambda: asyncio.run(orchestrator.process_urls([url]))
        )
        
        # Update final status
        if result and result[0].get("success"):
            task_store[task_id].update({
                "status": "completed",
                "progress": 100,
                "message": "Processing complete",
                "result": result[0]
            })
        else:
            task_store[task_id].update({
                "status": "failed",
                "message": result[0].get("error", "Unknown error") if result else "Failed"
            })
            
    except Exception as e:
        task_store[task_id].update({
            "status": "failed",
            "message": str(e)
        })
```

### 3.4 Update Pipeline V3 DB Client

Update `backend/pipeline_v3/services/db_client.py` to use Supabase:

```python
"""Database client for Pipeline V3 - Updated for Supabase."""

import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional
from supabase import create_client, Client
import os

from backend.pipeline_v3.models import (
    TranscriptAnalysis,
    BusinessPlan,
    VideoMetadata,
)
from backend.pipeline_v3.logger import logger

# Initialize Supabase client
SUPABASE_URL = os.getenv("SUPABASE_URL")
SUPABASE_KEY = os.getenv("SUPABASE_SERVICE_KEY")  # Service role for pipeline

supabase: Client = create_client(SUPABASE_URL, SUPABASE_KEY)

class DatabaseClient:
    """Client for persisting pipeline results to Supabase."""

    async def get_or_create_podcast(
        self, channel_url: str, metadata: Optional[VideoMetadata]
    ) -> Dict[str, Any]:
        """Get existing podcast or create a new one."""
        # Try to get existing podcast
        result = supabase.table("podcasts") \
            .select("*") \
            .eq("channel_url", channel_url) \
            .execute()
        
        if result.data:
            logger.debug(f"Found existing podcast: {result.data[0]['title']}")
            return result.data[0]
        
        # Create new podcast
        logger.info(f"Creating new podcast for channel: {channel_url}")
        podcast_data = {
            "id": str(uuid.uuid4()),
            "channel_url": channel_url,
            "title": metadata.channel_name if metadata else "Unknown Channel",
            "channel_name": metadata.channel_name if metadata else "Unknown",
            "description": "",
            "created_at": datetime.utcnow().isoformat(),
            "created_by": None,
        }
        
        result = supabase.table("podcasts").insert(podcast_data).execute()
        return result.data[0]

    async def create_episode(
        self,
        podcast_id: str,
        youtube_url: str,
        metadata: Optional[VideoMetadata],
        transcript: Optional[Any],
        analysis: Optional[TranscriptAnalysis],
    ) -> Dict[str, Any]:
        """Create a new episode."""
        logger.info(f"Creating episode for URL: {youtube_url}")
        
        # Extract transcript text
        transcript_text = None
        if transcript is not None:
            if hasattr(transcript, "full_text"):
                transcript_text = transcript.full_text
            elif isinstance(transcript, str):
                transcript_text = transcript
        
        # Parse upload_date
        published_date = None
        if metadata and metadata.upload_date:
            try:
                published_date = datetime.strptime(metadata.upload_date, "%Y%m%d").isoformat()
            except (ValueError, TypeError):
                pass
        
        episode_data = {
            "id": str(uuid.uuid4()),
            "podcast_id": podcast_id,
            "youtube_url": youtube_url,
            "title": metadata.title if metadata else "Untitled Episode",
            "description": metadata.description if metadata else "",
            "thumbnail_url": metadata.thumbnail_url if metadata else None,
            "transcript": transcript_text,
            "summary": analysis.summary if analysis else None,
            "published_date": published_date,
            "created_at": datetime.utcnow().isoformat(),
            "episode_metadata": metadata.model_dump() if metadata else {},
        }
        
        result = supabase.table("episodes").insert(episode_data).execute()
        return result.data[0]

    async def create_ideas(
        self, episode_id: str, analysis: TranscriptAnalysis
    ) -> List[str]:
        """Create ideas from business opportunities."""
        if not analysis or not analysis.business_opportunities:
            return []
        
        idea_ids = []
        ideas_data = []
        
        for opportunity in analysis.business_opportunities:
            idea_id = str(uuid.uuid4())
            idea_ids.append(idea_id)
            
            ideas_data.append({
                "id": idea_id,
                "episode_id": episode_id,
                "title": opportunity.title,
                "short_description": opportunity.short_description,
                "detailed_description": opportunity.detailed_description,
                "business_area": opportunity.business_area,
                "market_size": opportunity.estimated_market_size,
                "normalized_market_size_usd": opportunity.estimated_market_size,
                "created_at": datetime.utcnow().isoformat(),
            })
        
        if ideas_data:
            supabase.table("ideas").insert(ideas_data).execute()
            logger.info(f"Created {len(idea_ids)} ideas for episode {episode_id}")
        
        return idea_ids

    async def create_business_plans(
        self, idea_ids: List[str], business_plans: List[BusinessPlan]
    ) -> List[str]:
        """Create business plans linked to ideas."""
        if not business_plans or not idea_ids:
            return []
        
        plan_ids = []
        plans_data = []
        
        for idx, (idea_id, plan) in enumerate(zip(idea_ids, business_plans)):
            plan_id = str(uuid.uuid4())
            plan_ids.append(plan_id)
            
            relevant_quotes_json = [
                {"text": q.text, "timestamp_seconds": q.timestamp_seconds}
                for q in plan.relevant_quotes
            ]
            
            plans_data.append({
                "id": plan_id,
                "idea_id": idea_id,
                "name": plan.overview.name,
                "short_description": plan.overview.short_description,
                "detailed_description": plan.overview.detailed_description,
                "value_propositions": [vp.model_dump() for vp in plan.overview.value_propositions],
                "challenges": [c.model_dump() for c in plan.overview.challenges],
                "opportunities": [o.model_dump() for o in plan.overview.opportunities],
                "target_market": plan.business_plan.target_market,
                "revenue_model": plan.business_plan.revenue_model,
                "foundation_setup": plan.implementation.foundation_setup.model_dump(),
                "program_development": plan.implementation.program_development.model_dump(),
                "market_size": plan.key_metrics.market_size,
                "timeline": plan.key_metrics.timeline,
                "initial_investment": plan.key_metrics.initial_investment,
                "currency": plan.key_metrics.currency,
                "team_size": plan.key_metrics.team_size,
                "competition_level": plan.key_metrics.competition_level,
                "market_trend": plan.key_metrics.market_trend,
                "relevant_quotes": relevant_quotes_json,
                "created_at": datetime.utcnow().isoformat(),
            })
        
        if plans_data:
            supabase.table("business_plans").insert(plans_data).execute()
            logger.info(f"Created {len(plan_ids)} business plans")
        
        return plan_ids

    async def create_quotes(
        self, episode_id: str, analysis: TranscriptAnalysis
    ) -> List[str]:
        """Create quotes from analysis."""
        if not analysis or not analysis.notable_quotes:
            return []
        
        quote_ids = []
        quotes_data = []
        
        for quote_data in analysis.notable_quotes:
            quote_id = str(uuid.uuid4())
            quote_ids.append(quote_id)
            
            # Format timestamp
            display_timestamp = quote_data.timestamp
            if not display_timestamp and quote_data.timestamp_seconds is not None:
                secs = quote_data.timestamp_seconds
                hours = int(secs // 3600)
                minutes = int((secs % 3600) // 60)
                seconds = int(secs % 60)
                if hours > 0:
                    display_timestamp = f"{hours}:{minutes:02d}:{seconds:02d}"
                else:
                    display_timestamp = f"{minutes}:{seconds:02d}"
            
            quotes_data.append({
                "id": quote_id,
                "episode_id": episode_id,
                "text": quote_data.text,
                "speaker": quote_data.speaker,
                "context": quote_data.context,
                "timestamp": display_timestamp,
                "timestamp_seconds": quote_data.timestamp_seconds,
                "topics": quote_data.topics,
                "created_at": datetime.utcnow().isoformat(),
            })
        
        if quotes_data:
            supabase.table("quotes").insert(quotes_data).execute()
            logger.info(f"Created {len(quote_ids)} quotes for episode {episode_id}")
        
        return quote_ids

    async def create_insights(
        self, episode_id: str, analysis: TranscriptAnalysis
    ) -> List[str]:
        """Create insights from analysis."""
        if not analysis or not analysis.insights:
            return []
        
        insight_ids = []
        insights_data = []
        
        for insight_data in analysis.insights:
            insight_id = str(uuid.uuid4())
            insight_ids.append(insight_id)
            
            supporting_quotes_json = [
                {"text": q.text, "timestamp_seconds": q.timestamp_seconds}
                for q in insight_data.supporting_quotes
            ]
            
            insights_data.append({
                "id": insight_id,
                "episode_id": episode_id,
                "title": insight_data.title,
                "description": insight_data.description,
                "related_topics": insight_data.related_topics,
                "supporting_quotes": supporting_quotes_json,
                "timestamp_seconds": insight_data.timestamp_seconds,
                "created_at": datetime.utcnow().isoformat(),
            })
        
        if insights_data:
            supabase.table("insights").insert(insights_data).execute()
            logger.info(f"Created {len(insight_ids)} insights for episode {episode_id}")
        
        return insight_ids

    async def create_topics(
        self, episode_id: str, analysis: TranscriptAnalysis
    ) -> List[str]:
        """Create topics from analysis."""
        if not analysis or not analysis.key_topics:
            return []
        
        topic_ids = []
        topics_data = []
        
        for topic_data in analysis.key_topics:
            topic_id = str(uuid.uuid4())
            topic_ids.append(topic_id)
            
            topics_data.append({
                "id": topic_id,
                "episode_id": episode_id,
                "name": topic_data.name,
                "description": topic_data.description,
                "relevance_score": topic_data.relevance_score,
                "subtopics": topic_data.subtopics,
                "created_at": datetime.utcnow().isoformat(),
            })
        
        if topics_data:
            supabase.table("topics").insert(topics_data).execute()
            logger.info(f"Created {len(topic_ids)} topics for episode {episode_id}")
        
        return topic_ids

# Global database client instance
db_client = DatabaseClient()
```

---

## Phase 4: Chat Service Updates (Day 2 - Afternoon)

### 4.1 Update Chat to Use Supabase

The chat service needs to:
1. Query transcript chunks from Supabase (using the vector search function)
2. Store chat sessions/messages (can stay in SQLite or move to Supabase)

Update `app/services/chat/chat_orchestrator.py`:

```python
# Add Supabase client for vector queries
from supabase import create_client, Client
import os

supabase: Client = create_client(
    os.getenv("SUPABASE_URL"),
    os.getenv("SUPABASE_SERVICE_KEY")
)

# In the retrieval method:
async def retrieve_relevant_segments(
    self, 
    episode_id: str, 
    query: str, 
    limit: int = 5
) -> List[RetrievedSegment]:
    """Retrieve relevant transcript segments using vector similarity."""
    
    # Generate embedding for query
    query_embedding = await self.embeddings.generate_embedding(query)
    
    # Call Supabase RPC for vector similarity search
    result = supabase.rpc(
        "match_transcript_chunks",
        {
            "query_embedding": query_embedding,
            "match_threshold": 0.7,
            "match_count": limit,
            "episode_filter": episode_id
        }
    ).execute()
    
    # Convert to RetrievedSegment objects
    segments = []
    for row in result.data:
        segments.append(RetrievedSegment(
            text=row["content"],
            start_time=row["start_time"],
            end_time=row["end_time"],
            speaker=row["speaker"],
            similarity=row["similarity"]
        ))
    
    return segments
```

### 4.2 Update Transcript Ingestion

Update `app/services/transcript_ingestion.py` to store embeddings in Supabase:

```python
async def ingest_transcript(
    self, 
    db: AsyncSession,  # Keep for chat_sessions reference
    episode_id: str, 
    transcript: str
) -> int:
    """Ingest transcript into Supabase with embeddings."""
    
    # Chunk the transcript
    chunks = self.chunk_transcript(transcript)
    
    # Generate embeddings and store in Supabase
    chunk_data = []
    for idx, chunk in enumerate(chunks):
        embedding = await self.embeddings.generate_embedding(chunk["text"])
        
        chunk_data.append({
            "episode_id": episode_id,
            "chunk_index": idx,
            "content": chunk["text"],
            "embedding": embedding,
            "start_time": chunk["start_time"],
            "end_time": chunk["end_time"],
            "speaker": chunk.get("speaker")
        })
    
    # Batch insert to Supabase
    supabase.table("transcript_chunks").insert(chunk_data).execute()
    
    return len(chunks)
```

---

## Phase 5: Deployment & Cutover (Day 2 - Evening)

### 5.1 Pre-Cutover Checklist

- [ ] All data migrated to Supabase
- [ ] Frontend updated to use Supabase client
- [ ] Backend slimmed down to chat + pipeline only
- [ ] Pipeline V3 updated to write to Supabase
- [ ] Environment variables configured:
  - Frontend: `VITE_SUPABASE_URL`, `VITE_SUPABASE_ANON_KEY`
  - Backend: `SUPABASE_URL`, `SUPABASE_SERVICE_KEY`
- [ ] RLS policies tested
- [ ] Backup of old database created

### 5.2 Cutover Steps

1. **Put site in maintenance mode** (30 minutes)
   - Show maintenance page on frontend
   - Stop all new writes to old database

2. **Final data sync** (15 minutes)
   - Run migration script one last time for any new data
   - Verify row counts match

3. **Deploy new frontend** (15 minutes)
   - Deploy to Netlify with new Supabase client code
   - Verify all pages load correctly

4. **Deploy new backend** (15 minutes)
   - Deploy slimmed backend (chat + pipeline only)
   - Verify chat endpoints work
   - Test pipeline trigger

5. **Remove maintenance mode**
   - Monitor for errors
   - Keep old database for 1 week (rollback safety)

### 5.3 Rollback Plan

If issues occur:
1. Put site in maintenance mode
2. Switch frontend back to old API
3. Restore old backend deployment
4. Remove maintenance mode
5. Investigate and fix issues

---

## Summary of Changes

### Frontend Changes

| File | Change |
|------|--------|
| `src/services/supabase.ts` | **NEW** - All data queries |
| `src/services/api.ts` | **MODIFY** - Only chat + pipeline |
| `src/pages/*.tsx` | **MODIFY** - Update imports |

### Backend Changes

| File | Change |
|------|--------|
| `main.py` | **MODIFY** - Remove most routers |
| `app/api/v1/endpoints/*.py` | **DELETE** - Remove all except chat.py, Create pipeline.py |
| `app/services/ideas.py` | **DELETE** - No longer needed |
| `app/services/episodes.py` | **DELETE** - No longer needed |
| `app/services/podcasts.py` | **DELETE** - No longer needed |
| `backend/pipeline_v3/services/db_client.py` | **MODIFY** - Use Supabase |

### Code Reduction Estimates

- **Backend**: 8,000 → 2,000 lines (-75%)
- **Frontend**: +500 lines (new Supabase service)
- **Database**: Single source of truth in Supabase

### What Stays in Backend

1. **Chat/RAG** (`app/services/chat/`)
   - Chat orchestration
   - Vector retrieval
   - LLM integration
   - Session management

2. **Pipeline V3** (`backend/pipeline_v3/`)
   - YouTube processing
   - Transcription
   - AI analysis
   - Writes to Supabase

3. **Transcript Indexing** (`app/services/transcript_ingestion.py`)
   - Chunking
   - Embedding generation
   - Storage in Supabase

---

## Post-Migration Benefits

1. **Simplified Architecture**
   - Single database (Supabase)
   - No double user storage
   - No auth webhook needed

2. **Performance**
   - Direct queries from frontend
   - No API round-trip for reads
   - Built-in caching

3. **Features** (Free with Supabase)
   - Real-time subscriptions
   - Built-in auth
   - Auto-generated APIs
   - Row Level Security
   - Connection pooling
   - Backups

4. **Cost**
   - Reduced backend hosting costs
   - Potentially free tier viable
   - No Redis needed (unless for chat sessions)

5. **Maintenance**
   - ~75% less backend code
   - No database migrations to manage
   - Simpler deployment

---

## Questions?

Ready to proceed? I can help you:
1. Create detailed migration scripts
2. Write the frontend Supabase service layer
3. Update the Pipeline V3 DB client
4. Create the slim backend
5. Plan the deployment sequence

Just let me know which phase you'd like to start with!