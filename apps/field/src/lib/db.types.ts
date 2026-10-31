export type Json =
  | string
  | number
  | boolean
  | null
  | { [key: string]: Json | undefined }
  | Json[]

export type Database = {
  // Allows to automatically instantiate createClient with right options
  // instead of createClient<Database, { PostgrestVersion: 'XX' }>(URL, KEY)
  __InternalSupabase: {
    PostgrestVersion: "14.17"
  }
  graphql_public: {
    Tables: {
      [_ in never]: never
    }
    Views: {
      [_ in never]: never
    }
    Functions: {
      graphql: {
        Args: {
          extensions?: Json
          operationName?: string
          query?: string
          variables?: Json
        }
        Returns: Json
      }
    }
    Enums: {
      [_ in never]: never
    }
    CompositeTypes: {
      [_ in never]: never
    }
  }
  public: {
    Tables: {
      app_user: {
        Row: {
          created_at: string
          display_name: string
          id: string
          locale: string
          must_change_password: boolean
          person_id: string | null
          updated_at: string
        }
        Insert: {
          created_at?: string
          display_name: string
          id: string
          locale?: string
          must_change_password?: boolean
          person_id?: string | null
          updated_at?: string
        }
        Update: {
          created_at?: string
          display_name?: string
          id?: string
          locale?: string
          must_change_password?: boolean
          person_id?: string | null
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "app_user_person_id_fkey"
            columns: ["person_id"]
            isOneToOne: false
            referencedRelation: "person"
            referencedColumns: ["id"]
          },
        ]
      }
      audit_log: {
        Row: {
          action: string
          actor: string | null
          after: Json | null
          before: Json | null
          id: number
          occurred_at: string
          record_id: string
          table_name: string
        }
        Insert: {
          action: string
          actor?: string | null
          after?: Json | null
          before?: Json | null
          id?: number
          occurred_at?: string
          record_id: string
          table_name: string
        }
        Update: {
          action?: string
          actor?: string | null
          after?: Json | null
          before?: Json | null
          id?: number
          occurred_at?: string
          record_id?: string
          table_name?: string
        }
        Relationships: []
      }
      bug_report: {
        Row: {
          actions: Json
          created_at: string
          description: string
          id: string
          photo_path: string | null
          ref: number
          reporter_id: string
          resolution: string | null
          route: string
          screenshot_path: string | null
          status: Database["public"]["Enums"]["bug_status"]
          updated_at: string
          user_agent: string
          viewport: string
        }
        Insert: {
          actions: Json
          created_at?: string
          description: string
          id?: string
          photo_path?: string | null
          ref?: never
          reporter_id?: string
          resolution?: string | null
          route: string
          screenshot_path?: string | null
          status?: Database["public"]["Enums"]["bug_status"]
          updated_at?: string
          user_agent: string
          viewport: string
        }
        Update: {
          actions?: Json
          created_at?: string
          description?: string
          id?: string
          photo_path?: string | null
          ref?: never
          reporter_id?: string
          resolution?: string | null
          route?: string
          screenshot_path?: string | null
          status?: Database["public"]["Enums"]["bug_status"]
          updated_at?: string
          user_agent?: string
          viewport?: string
        }
        Relationships: [
          {
            foreignKeyName: "bug_report_reporter_id_fkey"
            columns: ["reporter_id"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
        ]
      }
      buyer: {
        Row: {
          channel: Database["public"]["Enums"]["buyer_channel"]
          contact_note: string | null
          created_at: string
          id: string
          is_active: boolean
          name: string
          project_id: string
          updated_at: string
        }
        Insert: {
          channel?: Database["public"]["Enums"]["buyer_channel"]
          contact_note?: string | null
          created_at?: string
          id?: string
          is_active?: boolean
          name: string
          project_id: string
          updated_at?: string
        }
        Update: {
          channel?: Database["public"]["Enums"]["buyer_channel"]
          contact_note?: string | null
          created_at?: string
          id?: string
          is_active?: boolean
          name?: string
          project_id?: string
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "buyer_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
        ]
      }
      buyer_demand: {
        Row: {
          buyer_id: string
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          crop_id: string
          currency: string
          deleted_at: string | null
          delivery_point: string | null
          id: string
          indicative_price_per_kg: number | null
          project_id: string
          quality_note: string | null
          quantity_kg: number
          status: Database["public"]["Enums"]["demand_status"]
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          window_end: string
          window_start: string
        }
        Insert: {
          buyer_id: string
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          crop_id: string
          currency?: string
          deleted_at?: string | null
          delivery_point?: string | null
          id?: string
          indicative_price_per_kg?: number | null
          project_id: string
          quality_note?: string | null
          quantity_kg: number
          status?: Database["public"]["Enums"]["demand_status"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          window_end: string
          window_start: string
        }
        Update: {
          buyer_id?: string
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          crop_id?: string
          currency?: string
          deleted_at?: string | null
          delivery_point?: string | null
          id?: string
          indicative_price_per_kg?: number | null
          project_id?: string
          quality_note?: string | null
          quantity_kg?: number
          status?: Database["public"]["Enums"]["demand_status"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          window_end?: string
          window_start?: string
        }
        Relationships: [
          {
            foreignKeyName: "buyer_demand_buyer_id_fkey"
            columns: ["buyer_id"]
            isOneToOne: false
            referencedRelation: "buyer"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "buyer_demand_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "buyer_demand_crop_id_fkey"
            columns: ["crop_id"]
            isOneToOne: false
            referencedRelation: "crop"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "buyer_demand_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
        ]
      }
      country: {
        Row: {
          created_at: string
          default_area_unit: Database["public"]["Enums"]["area_unit"]
          id: string
          iso2: string
          name: string
          updated_at: string
        }
        Insert: {
          created_at?: string
          default_area_unit?: Database["public"]["Enums"]["area_unit"]
          id?: string
          iso2: string
          name: string
          updated_at?: string
        }
        Update: {
          created_at?: string
          default_area_unit?: Database["public"]["Enums"]["area_unit"]
          id?: string
          iso2?: string
          name?: string
          updated_at?: string
        }
        Relationships: []
      }
      crop: {
        Row: {
          code: string
          created_at: string
          id: string
          is_active: boolean
          measured_by: Database["public"]["Enums"]["crop_measure"]
          name_en: string
          name_sw: string
          updated_at: string
        }
        Insert: {
          code: string
          created_at?: string
          id?: string
          is_active?: boolean
          measured_by?: Database["public"]["Enums"]["crop_measure"]
          name_en: string
          name_sw: string
          updated_at?: string
        }
        Update: {
          code?: string
          created_at?: string
          id?: string
          is_active?: boolean
          measured_by?: Database["public"]["Enums"]["crop_measure"]
          name_en?: string
          name_sw?: string
          updated_at?: string
        }
        Relationships: []
      }
      crop_cycle: {
        Row: {
          area_ha: number | null
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          crop_id: string
          deleted_at: string | null
          evidence_ref: string | null
          harvest_end: string | null
          harvest_start: string | null
          id: string
          planted_on: string | null
          plot_id: string
          season_label: string | null
          source: Database["public"]["Enums"]["source_type"]
          status: Database["public"]["Enums"]["crop_cycle_status"]
          tree_count: number | null
          unit_count: number | null
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          area_ha?: number | null
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          crop_id: string
          deleted_at?: string | null
          evidence_ref?: string | null
          harvest_end?: string | null
          harvest_start?: string | null
          id?: string
          planted_on?: string | null
          plot_id: string
          season_label?: string | null
          source: Database["public"]["Enums"]["source_type"]
          status?: Database["public"]["Enums"]["crop_cycle_status"]
          tree_count?: number | null
          unit_count?: number | null
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          area_ha?: number | null
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          crop_id?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          harvest_end?: string | null
          harvest_start?: string | null
          id?: string
          planted_on?: string | null
          plot_id?: string
          season_label?: string | null
          source?: Database["public"]["Enums"]["source_type"]
          status?: Database["public"]["Enums"]["crop_cycle_status"]
          tree_count?: number | null
          unit_count?: number | null
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "crop_cycle_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "crop_cycle_crop_id_fkey"
            columns: ["crop_id"]
            isOneToOne: false
            referencedRelation: "crop"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "crop_cycle_plot_id_fkey"
            columns: ["plot_id"]
            isOneToOne: false
            referencedRelation: "plot"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "crop_cycle_verified_by_fkey"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "cycle_plot_village_fk"
            columns: ["plot_id", "village_id"]
            isOneToOne: false
            referencedRelation: "plot"
            referencedColumns: ["id", "village_id"]
          },
        ]
      }
      energy_estimate: {
        Row: {
          computed_at: string
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          days_per_week: number
          est_kwh_per_day: number | null
          est_kwh_per_week: number | null
          est_power_kw: number | null
          hours_per_day: number
          id: string
          method: string
          pue_request_id: string
          quantity: number
          rated_power_kw: number
          source: Database["public"]["Enums"]["source_type"]
          updated_at: string
          village_id: string
        }
        Insert: {
          computed_at?: string
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          days_per_week: number
          est_kwh_per_day?: number | null
          est_kwh_per_week?: number | null
          est_power_kw?: number | null
          hours_per_day: number
          id?: string
          method?: string
          pue_request_id: string
          quantity: number
          rated_power_kw: number
          source?: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          village_id: string
        }
        Update: {
          computed_at?: string
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          days_per_week?: number
          est_kwh_per_day?: number | null
          est_kwh_per_week?: number | null
          est_power_kw?: number | null
          hours_per_day?: number
          id?: string
          method?: string
          pue_request_id?: string
          quantity?: number
          rated_power_kw?: number
          source?: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "energy_estimate_pue_request_id_fkey"
            columns: ["pue_request_id"]
            isOneToOne: true
            referencedRelation: "pue_request"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "energy_estimate_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "energy_estimate_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "energy_estimate_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      equipment: {
        Row: {
          can_buy: boolean
          can_rent: boolean
          category_id: string
          code: string
          created_at: string
          currency: string
          id: string
          indicative_price: number | null
          indicative_rent_per_day: number | null
          is_active: boolean
          name_en: string
          name_sw: string
          project_id: string
          rated_power_kw: number | null
          typical_days_per_week: number | null
          typical_hours_per_day: number | null
          updated_at: string
        }
        Insert: {
          can_buy?: boolean
          can_rent?: boolean
          category_id: string
          code: string
          created_at?: string
          currency?: string
          id?: string
          indicative_price?: number | null
          indicative_rent_per_day?: number | null
          is_active?: boolean
          name_en: string
          name_sw: string
          project_id: string
          rated_power_kw?: number | null
          typical_days_per_week?: number | null
          typical_hours_per_day?: number | null
          updated_at?: string
        }
        Update: {
          can_buy?: boolean
          can_rent?: boolean
          category_id?: string
          code?: string
          created_at?: string
          currency?: string
          id?: string
          indicative_price?: number | null
          indicative_rent_per_day?: number | null
          is_active?: boolean
          name_en?: string
          name_sw?: string
          project_id?: string
          rated_power_kw?: number | null
          typical_days_per_week?: number | null
          typical_hours_per_day?: number | null
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "equipment_category_id_fkey"
            columns: ["category_id"]
            isOneToOne: false
            referencedRelation: "equipment_category"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "equipment_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
        ]
      }
      equipment_category: {
        Row: {
          code: string
          created_at: string
          id: string
          is_active: boolean
          name_en: string
          name_sw: string
          updated_at: string
        }
        Insert: {
          code: string
          created_at?: string
          id?: string
          is_active?: boolean
          name_en: string
          name_sw: string
          updated_at?: string
        }
        Update: {
          code?: string
          created_at?: string
          id?: string
          is_active?: boolean
          name_en?: string
          name_sw?: string
          updated_at?: string
        }
        Relationships: []
      }
      farm: {
        Row: {
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          deleted_at: string | null
          evidence_ref: string | null
          household_id: string | null
          id: string
          label: string
          latitude: number | null
          longitude: number | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          household_id?: string | null
          id?: string
          label: string
          latitude?: number | null
          longitude?: number | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          household_id?: string | null
          id?: string
          label?: string
          latitude?: number | null
          longitude?: number | null
          source?: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "farm_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "farm_household_id_fkey"
            columns: ["household_id"]
            isOneToOne: false
            referencedRelation: "household"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "farm_verified_by_fkey"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "farm_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "farm_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "farm_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      farm_manager: {
        Row: {
          created_at: string
          farm_id: string
          id: string
          is_primary: boolean
          person_id: string
        }
        Insert: {
          created_at?: string
          farm_id: string
          id?: string
          is_primary?: boolean
          person_id: string
        }
        Update: {
          created_at?: string
          farm_id?: string
          id?: string
          is_primary?: boolean
          person_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "farm_manager_farm_id_fkey"
            columns: ["farm_id"]
            isOneToOne: false
            referencedRelation: "farm"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "farm_manager_person_id_fkey"
            columns: ["person_id"]
            isOneToOne: false
            referencedRelation: "person"
            referencedColumns: ["id"]
          },
        ]
      }
      harvest_report: {
        Row: {
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          crop_cycle_id: string
          deleted_at: string | null
          evidence_ref: string | null
          id: string
          is_current: boolean
          kind: Database["public"]["Enums"]["harvest_kind"]
          quantity_kg: number
          reported_for: string | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          crop_cycle_id: string
          deleted_at?: string | null
          evidence_ref?: string | null
          id?: string
          is_current?: boolean
          kind: Database["public"]["Enums"]["harvest_kind"]
          quantity_kg: number
          reported_for?: string | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          crop_cycle_id?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          id?: string
          is_current?: boolean
          kind?: Database["public"]["Enums"]["harvest_kind"]
          quantity_kg?: number
          reported_for?: string | null
          source?: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "harvest_cycle_village_fk"
            columns: ["crop_cycle_id", "village_id"]
            isOneToOne: false
            referencedRelation: "crop_cycle"
            referencedColumns: ["id", "village_id"]
          },
          {
            foreignKeyName: "harvest_report_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "harvest_report_crop_cycle_id_fkey"
            columns: ["crop_cycle_id"]
            isOneToOne: false
            referencedRelation: "crop_cycle"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "harvest_report_verified_by_fkey"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
        ]
      }
      household: {
        Row: {
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          deleted_at: string | null
          evidence_ref: string | null
          id: string
          label: string
          source: Database["public"]["Enums"]["source_type"]
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          id?: string
          label: string
          source: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          id?: string
          label?: string
          source?: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "household_captured_by_fk"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "household_verified_by_fk"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "household_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "household_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "household_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      household_member: {
        Row: {
          created_at: string
          household_id: string
          id: string
          is_head: boolean
          person_id: string
        }
        Insert: {
          created_at?: string
          household_id: string
          id?: string
          is_head?: boolean
          person_id: string
        }
        Update: {
          created_at?: string
          household_id?: string
          id?: string
          is_head?: boolean
          person_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "household_member_household_id_fkey"
            columns: ["household_id"]
            isOneToOne: false
            referencedRelation: "household"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "household_member_person_id_fkey"
            columns: ["person_id"]
            isOneToOne: false
            referencedRelation: "person"
            referencedColumns: ["id"]
          },
        ]
      }
      loan_product: {
        Row: {
          code: string
          created_at: string
          currency: string
          description_en: string | null
          description_sw: string | null
          id: string
          indicative_max_amount: number
          indicative_min_amount: number
          is_active: boolean
          name_en: string
          name_sw: string
          project_id: string
          updated_at: string
        }
        Insert: {
          code: string
          created_at?: string
          currency?: string
          description_en?: string | null
          description_sw?: string | null
          id?: string
          indicative_max_amount: number
          indicative_min_amount: number
          is_active?: boolean
          name_en: string
          name_sw: string
          project_id: string
          updated_at?: string
        }
        Update: {
          code?: string
          created_at?: string
          currency?: string
          description_en?: string | null
          description_sw?: string | null
          id?: string
          indicative_max_amount?: number
          indicative_min_amount?: number
          is_active?: boolean
          name_en?: string
          name_sw?: string
          project_id?: string
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "loan_product_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
        ]
      }
      login_issue: {
        Row: {
          id: string
          issued_at: string
          issued_by: string
          kind: string
          person_id: string
          temp_password_hash: string
          user_id: string
        }
        Insert: {
          id?: string
          issued_at?: string
          issued_by: string
          kind: string
          person_id: string
          temp_password_hash: string
          user_id: string
        }
        Update: {
          id?: string
          issued_at?: string
          issued_by?: string
          kind?: string
          person_id?: string
          temp_password_hash?: string
          user_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "login_issue_issued_by_fkey"
            columns: ["issued_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "login_issue_person_id_fkey"
            columns: ["person_id"]
            isOneToOne: false
            referencedRelation: "person"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "login_issue_user_id_fkey"
            columns: ["user_id"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
        ]
      }
      membership: {
        Row: {
          created_at: string
          id: string
          project_id: string
          revoked_at: string | null
          role: Database["public"]["Enums"]["app_role"]
          user_id: string
          village_id: string | null
        }
        Insert: {
          created_at?: string
          id?: string
          project_id: string
          revoked_at?: string | null
          role: Database["public"]["Enums"]["app_role"]
          user_id: string
          village_id?: string | null
        }
        Update: {
          created_at?: string
          id?: string
          project_id?: string
          revoked_at?: string | null
          role?: Database["public"]["Enums"]["app_role"]
          user_id?: string
          village_id?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "membership_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "membership_user_id_fkey"
            columns: ["user_id"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "membership_village_in_project"
            columns: ["village_id", "project_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id", "project_id"]
          },
        ]
      }
      opportunity: {
        Row: {
          buyer_demand_id: string
          captured_at: string
          captured_by: string | null
          created_at: string
          crop_id: string
          deleted_at: string | null
          id: string
          note: string | null
          offered_quantity_kg: number
          status: Database["public"]["Enums"]["opportunity_status"]
          updated_at: string
          village_id: string
        }
        Insert: {
          buyer_demand_id: string
          captured_at?: string
          captured_by?: string | null
          created_at?: string
          crop_id: string
          deleted_at?: string | null
          id?: string
          note?: string | null
          offered_quantity_kg?: number
          status?: Database["public"]["Enums"]["opportunity_status"]
          updated_at?: string
          village_id: string
        }
        Update: {
          buyer_demand_id?: string
          captured_at?: string
          captured_by?: string | null
          created_at?: string
          crop_id?: string
          deleted_at?: string | null
          id?: string
          note?: string | null
          offered_quantity_kg?: number
          status?: Database["public"]["Enums"]["opportunity_status"]
          updated_at?: string
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "opportunity_buyer_demand_id_fkey"
            columns: ["buyer_demand_id"]
            isOneToOne: false
            referencedRelation: "buyer_demand"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "opportunity_buyer_demand_id_fkey"
            columns: ["buyer_demand_id"]
            isOneToOne: false
            referencedRelation: "v_demand_match"
            referencedColumns: ["buyer_demand_id"]
          },
          {
            foreignKeyName: "opportunity_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "opportunity_crop_id_fkey"
            columns: ["crop_id"]
            isOneToOne: false
            referencedRelation: "crop"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "opportunity_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "opportunity_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "opportunity_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      opportunity_supply: {
        Row: {
          captured_at: string
          captured_by: string | null
          contributed_kg: number
          created_at: string
          crop_cycle_id: string
          harvest_report_id: string
          id: string
          opportunity_id: string
        }
        Insert: {
          captured_at?: string
          captured_by?: string | null
          contributed_kg: number
          created_at?: string
          crop_cycle_id: string
          harvest_report_id: string
          id?: string
          opportunity_id: string
        }
        Update: {
          captured_at?: string
          captured_by?: string | null
          contributed_kg?: number
          created_at?: string
          crop_cycle_id?: string
          harvest_report_id?: string
          id?: string
          opportunity_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "opportunity_supply_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "opportunity_supply_crop_cycle_id_fkey"
            columns: ["crop_cycle_id"]
            isOneToOne: false
            referencedRelation: "crop_cycle"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "opportunity_supply_harvest_report_id_fkey"
            columns: ["harvest_report_id"]
            isOneToOne: false
            referencedRelation: "harvest_report"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "opportunity_supply_harvest_report_id_fkey"
            columns: ["harvest_report_id"]
            isOneToOne: false
            referencedRelation: "v_harvest_available"
            referencedColumns: ["harvest_report_id"]
          },
          {
            foreignKeyName: "opportunity_supply_opportunity_id_fkey"
            columns: ["opportunity_id"]
            isOneToOne: false
            referencedRelation: "opportunity"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "opportunity_supply_opportunity_id_fkey"
            columns: ["opportunity_id"]
            isOneToOne: false
            referencedRelation: "v_demand_match"
            referencedColumns: ["opportunity_id"]
          },
        ]
      }
      person: {
        Row: {
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          deleted_at: string | null
          evidence_ref: string | null
          family_name: string
          given_name: string
          id: string
          phone: string | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          family_name: string
          given_name: string
          id?: string
          phone?: string | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          family_name?: string
          given_name?: string
          id?: string
          phone?: string | null
          source?: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "person_captured_by_fk"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "person_verified_by_fk"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "person_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "person_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "person_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      plot: {
        Row: {
          area_ha: number | null
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          deleted_at: string | null
          evidence_ref: string | null
          farm_id: string
          id: string
          label: string
          latitude: number | null
          longitude: number | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          area_ha?: number | null
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          farm_id: string
          id?: string
          label: string
          latitude?: number | null
          longitude?: number | null
          source: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          area_ha?: number | null
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          farm_id?: string
          id?: string
          label?: string
          latitude?: number | null
          longitude?: number | null
          source?: Database["public"]["Enums"]["source_type"]
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "plot_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "plot_farm_id_fkey"
            columns: ["farm_id"]
            isOneToOne: false
            referencedRelation: "farm"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "plot_farm_village_fk"
            columns: ["farm_id", "village_id"]
            isOneToOne: false
            referencedRelation: "farm"
            referencedColumns: ["id", "village_id"]
          },
          {
            foreignKeyName: "plot_verified_by_fkey"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
        ]
      }
      plot_document: {
        Row: {
          created_at: string
          file_name: string
          id: string
          mime_type: string
          plot_id: string
          size_bytes: number
          storage_path: string
          uploaded_at: string
          uploaded_by: string
          village_id: string
        }
        Insert: {
          created_at?: string
          file_name: string
          id?: string
          mime_type: string
          plot_id: string
          size_bytes: number
          storage_path: string
          uploaded_at?: string
          uploaded_by?: string
          village_id: string
        }
        Update: {
          created_at?: string
          file_name?: string
          id?: string
          mime_type?: string
          plot_id?: string
          size_bytes?: number
          storage_path?: string
          uploaded_at?: string
          uploaded_by?: string
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "plot_document_plot_fk"
            columns: ["plot_id", "village_id"]
            isOneToOne: false
            referencedRelation: "plot"
            referencedColumns: ["id", "village_id"]
          },
          {
            foreignKeyName: "plot_document_uploaded_by_fkey"
            columns: ["uploaded_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
        ]
      }
      project: {
        Row: {
          code: string
          country_id: string
          created_at: string
          id: string
          name: string
          operator: string | null
          started_on: string | null
          status: Database["public"]["Enums"]["project_status"]
          updated_at: string
        }
        Insert: {
          code: string
          country_id: string
          created_at?: string
          id?: string
          name: string
          operator?: string | null
          started_on?: string | null
          status?: Database["public"]["Enums"]["project_status"]
          updated_at?: string
        }
        Update: {
          code?: string
          country_id?: string
          created_at?: string
          id?: string
          name?: string
          operator?: string | null
          started_on?: string | null
          status?: Database["public"]["Enums"]["project_status"]
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "project_country_id_fkey"
            columns: ["country_id"]
            isOneToOne: false
            referencedRelation: "country"
            referencedColumns: ["id"]
          },
        ]
      }
      pue_request: {
        Row: {
          acquisition: Database["public"]["Enums"]["acquisition_mode"]
          captured_at: string
          captured_by: string | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          days_per_week: number | null
          decided_at: string | null
          decided_by: string | null
          decision_note: string | null
          deleted_at: string | null
          equipment_id: string
          evidence_ref: string | null
          farm_id: string | null
          hours_per_day: number | null
          id: string
          person_id: string
          purpose: string | null
          quantity: number
          source: Database["public"]["Enums"]["source_type"]
          status: Database["public"]["Enums"]["pue_status"]
          submitted_at: string | null
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          acquisition?: Database["public"]["Enums"]["acquisition_mode"]
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          days_per_week?: number | null
          decided_at?: string | null
          decided_by?: string | null
          decision_note?: string | null
          deleted_at?: string | null
          equipment_id: string
          evidence_ref?: string | null
          farm_id?: string | null
          hours_per_day?: number | null
          id?: string
          person_id: string
          purpose?: string | null
          quantity?: number
          source?: Database["public"]["Enums"]["source_type"]
          status?: Database["public"]["Enums"]["pue_status"]
          submitted_at?: string | null
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          acquisition?: Database["public"]["Enums"]["acquisition_mode"]
          captured_at?: string
          captured_by?: string | null
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          days_per_week?: number | null
          decided_at?: string | null
          decided_by?: string | null
          decision_note?: string | null
          deleted_at?: string | null
          equipment_id?: string
          evidence_ref?: string | null
          farm_id?: string | null
          hours_per_day?: number | null
          id?: string
          person_id?: string
          purpose?: string | null
          quantity?: number
          source?: Database["public"]["Enums"]["source_type"]
          status?: Database["public"]["Enums"]["pue_status"]
          submitted_at?: string | null
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "pue_farm_village_fk"
            columns: ["farm_id", "village_id"]
            isOneToOne: false
            referencedRelation: "farm"
            referencedColumns: ["id", "village_id"]
          },
          {
            foreignKeyName: "pue_request_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "pue_request_decided_by_fkey"
            columns: ["decided_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "pue_request_equipment_id_fkey"
            columns: ["equipment_id"]
            isOneToOne: false
            referencedRelation: "equipment"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "pue_request_person_id_fkey"
            columns: ["person_id"]
            isOneToOne: false
            referencedRelation: "person"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "pue_request_verified_by_fkey"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "pue_request_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "pue_request_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "pue_request_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      registration_receipt: {
        Row: {
          client_ref: string
          created_at: string
          created_by: string
          result: Json
        }
        Insert: {
          client_ref: string
          created_at?: string
          created_by: string
          result: Json
        }
        Update: {
          client_ref?: string
          created_at?: string
          created_by?: string
          result?: Json
        }
        Relationships: [
          {
            foreignKeyName: "registration_receipt_created_by_fkey"
            columns: ["created_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
        ]
      }
      survey: {
        Row: {
          audit_rate: number
          closed_at: string | null
          closes_at: string | null
          created_at: string
          created_by: string | null
          currency: string
          description_en: string | null
          description_sw: string | null
          id: string
          max_households: number | null
          project_id: string
          published_at: string | null
          published_by: string | null
          reward_amount: number
          status: Database["public"]["Enums"]["survey_status"]
          title_en: string
          title_sw: string | null
          updated_at: string
          village_id: string | null
        }
        Insert: {
          audit_rate?: number
          closed_at?: string | null
          closes_at?: string | null
          created_at?: string
          created_by?: string | null
          currency?: string
          description_en?: string | null
          description_sw?: string | null
          id?: string
          max_households?: number | null
          project_id: string
          published_at?: string | null
          published_by?: string | null
          reward_amount: number
          status?: Database["public"]["Enums"]["survey_status"]
          title_en: string
          title_sw?: string | null
          updated_at?: string
          village_id?: string | null
        }
        Update: {
          audit_rate?: number
          closed_at?: string | null
          closes_at?: string | null
          created_at?: string
          created_by?: string | null
          currency?: string
          description_en?: string | null
          description_sw?: string | null
          id?: string
          max_households?: number | null
          project_id?: string
          published_at?: string | null
          published_by?: string | null
          reward_amount?: number
          status?: Database["public"]["Enums"]["survey_status"]
          title_en?: string
          title_sw?: string | null
          updated_at?: string
          village_id?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "survey_created_by_fkey"
            columns: ["created_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_published_by_fkey"
            columns: ["published_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_village_in_project"
            columns: ["village_id", "project_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id", "project_id"]
          },
        ]
      }
      survey_answer: {
        Row: {
          created_at: string
          id: string
          question_id: string
          response_id: string
          value: Json
        }
        Insert: {
          created_at?: string
          id?: string
          question_id: string
          response_id: string
          value: Json
        }
        Update: {
          created_at?: string
          id?: string
          question_id?: string
          response_id?: string
          value?: Json
        }
        Relationships: [
          {
            foreignKeyName: "survey_answer_question_id_fkey"
            columns: ["question_id"]
            isOneToOne: false
            referencedRelation: "survey_question"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_answer_question_id_fkey"
            columns: ["question_id"]
            isOneToOne: false
            referencedRelation: "v_survey_answer_tally"
            referencedColumns: ["question_id"]
          },
          {
            foreignKeyName: "survey_answer_question_id_fkey"
            columns: ["question_id"]
            isOneToOne: false
            referencedRelation: "v_survey_number_summary"
            referencedColumns: ["question_id"]
          },
          {
            foreignKeyName: "survey_answer_response_id_fkey"
            columns: ["response_id"]
            isOneToOne: false
            referencedRelation: "survey_response"
            referencedColumns: ["id"]
          },
        ]
      }
      survey_question: {
        Row: {
          created_at: string
          deleted_at: string | null
          id: string
          kind: Database["public"]["Enums"]["survey_question_kind"]
          options: Json
          position: number
          prompt_en: string
          prompt_sw: string | null
          required: boolean
          survey_id: string
          updated_at: string
        }
        Insert: {
          created_at?: string
          deleted_at?: string | null
          id?: string
          kind: Database["public"]["Enums"]["survey_question_kind"]
          options?: Json
          position: number
          prompt_en: string
          prompt_sw?: string | null
          required?: boolean
          survey_id: string
          updated_at?: string
        }
        Update: {
          created_at?: string
          deleted_at?: string | null
          id?: string
          kind?: Database["public"]["Enums"]["survey_question_kind"]
          options?: Json
          position?: number
          prompt_en?: string
          prompt_sw?: string | null
          required?: boolean
          survey_id?: string
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "survey_question_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "survey"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_question_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "v_survey_summary"
            referencedColumns: ["survey_id"]
          },
        ]
      }
      survey_response: {
        Row: {
          captured_at: string
          captured_by: string | null
          client_ref: string
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          created_at: string
          deleted_at: string | null
          evidence_ref: string | null
          household_id: string
          id: string
          person_id: string
          source: Database["public"]["Enums"]["source_type"]
          submitted_at: string
          survey_id: string
          updated_at: string
          verification: Database["public"]["Enums"]["verification_status"]
          verified_at: string | null
          verified_by: string | null
          village_id: string
        }
        Insert: {
          captured_at?: string
          captured_by?: string | null
          client_ref: string
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          household_id: string
          id?: string
          person_id: string
          source: Database["public"]["Enums"]["source_type"]
          submitted_at?: string
          survey_id: string
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id: string
        }
        Update: {
          captured_at?: string
          captured_by?: string | null
          client_ref?: string
          confidence?: Database["public"]["Enums"]["confidence_level"] | null
          created_at?: string
          deleted_at?: string | null
          evidence_ref?: string | null
          household_id?: string
          id?: string
          person_id?: string
          source?: Database["public"]["Enums"]["source_type"]
          submitted_at?: string
          survey_id?: string
          updated_at?: string
          verification?: Database["public"]["Enums"]["verification_status"]
          verified_at?: string | null
          verified_by?: string | null
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "survey_response_captured_by_fkey"
            columns: ["captured_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_response_household_id_fkey"
            columns: ["household_id"]
            isOneToOne: false
            referencedRelation: "household"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_response_person_id_fkey"
            columns: ["person_id"]
            isOneToOne: false
            referencedRelation: "person"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_response_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "survey"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_response_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "v_survey_summary"
            referencedColumns: ["survey_id"]
          },
          {
            foreignKeyName: "survey_response_verified_by_fkey"
            columns: ["verified_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_response_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "survey_response_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "survey_response_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      survey_voucher: {
        Row: {
          amount: number
          audit_required: boolean
          code: string
          created_at: string
          currency: string
          expires_at: string
          household_id: string
          id: string
          id_type_seen: Database["public"]["Enums"]["id_document_type"] | null
          issued_at: string
          redeemed_at: string | null
          redeemed_by: string | null
          response_id: string
          status: Database["public"]["Enums"]["voucher_status"]
          survey_id: string
          updated_at: string
          village_id: string
          void_reason: string | null
          voided_at: string | null
          voided_by: string | null
        }
        Insert: {
          amount: number
          audit_required?: boolean
          code: string
          created_at?: string
          currency: string
          expires_at: string
          household_id: string
          id?: string
          id_type_seen?: Database["public"]["Enums"]["id_document_type"] | null
          issued_at?: string
          redeemed_at?: string | null
          redeemed_by?: string | null
          response_id: string
          status?: Database["public"]["Enums"]["voucher_status"]
          survey_id: string
          updated_at?: string
          village_id: string
          void_reason?: string | null
          voided_at?: string | null
          voided_by?: string | null
        }
        Update: {
          amount?: number
          audit_required?: boolean
          code?: string
          created_at?: string
          currency?: string
          expires_at?: string
          household_id?: string
          id?: string
          id_type_seen?: Database["public"]["Enums"]["id_document_type"] | null
          issued_at?: string
          redeemed_at?: string | null
          redeemed_by?: string | null
          response_id?: string
          status?: Database["public"]["Enums"]["voucher_status"]
          survey_id?: string
          updated_at?: string
          village_id?: string
          void_reason?: string | null
          voided_at?: string | null
          voided_by?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "survey_voucher_household_id_fkey"
            columns: ["household_id"]
            isOneToOne: false
            referencedRelation: "household"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_voucher_redeemed_by_fkey"
            columns: ["redeemed_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_voucher_response_id_fkey"
            columns: ["response_id"]
            isOneToOne: true
            referencedRelation: "survey_response"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_voucher_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "survey"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_voucher_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "v_survey_summary"
            referencedColumns: ["survey_id"]
          },
          {
            foreignKeyName: "survey_voucher_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "survey_voucher_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "survey_voucher_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_voucher_voided_by_fkey"
            columns: ["voided_by"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
        ]
      }
      village: {
        Row: {
          code: string
          created_at: string
          id: string
          latitude: number | null
          longitude: number | null
          name: string
          primary_language: string
          project_id: string
          updated_at: string
        }
        Insert: {
          code: string
          created_at?: string
          id?: string
          latitude?: number | null
          longitude?: number | null
          name: string
          primary_language?: string
          project_id: string
          updated_at?: string
        }
        Update: {
          code?: string
          created_at?: string
          id?: string
          latitude?: number | null
          longitude?: number | null
          name?: string
          primary_language?: string
          project_id?: string
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "village_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
        ]
      }
      village_capacity: {
        Row: {
          basis: Database["public"]["Enums"]["capacity_basis"]
          capacity_kw: number
          created_at: string
          effective_from: string
          id: string
          is_current: boolean
          simultaneity_factor: number
          source_note: string | null
          updated_at: string
          village_id: string
        }
        Insert: {
          basis?: Database["public"]["Enums"]["capacity_basis"]
          capacity_kw: number
          created_at?: string
          effective_from?: string
          id?: string
          is_current?: boolean
          simultaneity_factor?: number
          source_note?: string | null
          updated_at?: string
          village_id: string
        }
        Update: {
          basis?: Database["public"]["Enums"]["capacity_basis"]
          capacity_kw?: number
          created_at?: string
          effective_from?: string
          id?: string
          is_current?: boolean
          simultaneity_factor?: number
          source_note?: string | null
          updated_at?: string
          village_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "village_capacity_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "village_capacity_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "village_capacity_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      voucher_event: {
        Row: {
          actor_id: string | null
          actor_name: string
          actor_role: string
          detail: Json
          id: string
          kind: Database["public"]["Enums"]["voucher_event_kind"]
          occurred_at: string
          voucher_id: string
        }
        Insert: {
          actor_id?: string | null
          actor_name: string
          actor_role: string
          detail?: Json
          id?: string
          kind: Database["public"]["Enums"]["voucher_event_kind"]
          occurred_at?: string
          voucher_id: string
        }
        Update: {
          actor_id?: string | null
          actor_name?: string
          actor_role?: string
          detail?: Json
          id?: string
          kind?: Database["public"]["Enums"]["voucher_event_kind"]
          occurred_at?: string
          voucher_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "voucher_event_actor_id_fkey"
            columns: ["actor_id"]
            isOneToOne: false
            referencedRelation: "app_user"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "voucher_event_voucher_id_fkey"
            columns: ["voucher_id"]
            isOneToOne: false
            referencedRelation: "survey_voucher"
            referencedColumns: ["id"]
          },
        ]
      }
    }
    Views: {
      v_demand_match: {
        Row: {
          available_kg: number | null
          buyer_demand_id: string | null
          buyer_id: string | null
          committed_kg: number | null
          coverable_kg: number | null
          coverage_pct: number | null
          crop_id: string | null
          demand_kg: number | null
          opportunity_id: string | null
          opportunity_status:
            | Database["public"]["Enums"]["opportunity_status"]
            | null
          village_id: string | null
          window_end: string | null
          window_start: string | null
        }
        Relationships: [
          {
            foreignKeyName: "buyer_demand_buyer_id_fkey"
            columns: ["buyer_id"]
            isOneToOne: false
            referencedRelation: "buyer"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "buyer_demand_crop_id_fkey"
            columns: ["crop_id"]
            isOneToOne: false
            referencedRelation: "crop"
            referencedColumns: ["id"]
          },
        ]
      }
      v_harvest_available: {
        Row: {
          available_kg: number | null
          committed_kg: number | null
          confidence: Database["public"]["Enums"]["confidence_level"] | null
          crop_cycle_id: string | null
          crop_id: string | null
          harvest_end: string | null
          harvest_report_id: string | null
          harvest_start: string | null
          plot_id: string | null
          quantity_kg: number | null
          verification:
            | Database["public"]["Enums"]["verification_status"]
            | null
          village_id: string | null
        }
        Relationships: [
          {
            foreignKeyName: "crop_cycle_crop_id_fkey"
            columns: ["crop_id"]
            isOneToOne: false
            referencedRelation: "crop"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "crop_cycle_plot_id_fkey"
            columns: ["plot_id"]
            isOneToOne: false
            referencedRelation: "plot"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "harvest_cycle_village_fk"
            columns: ["crop_cycle_id", "village_id"]
            isOneToOne: false
            referencedRelation: "crop_cycle"
            referencedColumns: ["id", "village_id"]
          },
          {
            foreignKeyName: "harvest_report_crop_cycle_id_fkey"
            columns: ["crop_cycle_id"]
            isOneToOne: false
            referencedRelation: "crop_cycle"
            referencedColumns: ["id"]
          },
        ]
      }
      v_survey_answer_tally: {
        Row: {
          answer_count: number | null
          kind: Database["public"]["Enums"]["survey_question_kind"] | null
          option_value: string | null
          position: number | null
          question_id: string | null
          survey_id: string | null
        }
        Relationships: [
          {
            foreignKeyName: "survey_question_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "survey"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_question_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "v_survey_summary"
            referencedColumns: ["survey_id"]
          },
        ]
      }
      v_survey_number_summary: {
        Row: {
          answer_count: number | null
          average: number | null
          maximum: number | null
          minimum: number | null
          position: number | null
          question_id: string | null
          survey_id: string | null
        }
        Relationships: [
          {
            foreignKeyName: "survey_question_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "survey"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "survey_question_survey_id_fkey"
            columns: ["survey_id"]
            isOneToOne: false
            referencedRelation: "v_survey_summary"
            referencedColumns: ["survey_id"]
          },
        ]
      }
      v_survey_summary: {
        Row: {
          expired_count: number | null
          issued_amount: number | null
          issued_count: number | null
          outstanding_amount: number | null
          outstanding_count: number | null
          project_id: string | null
          redeemed_amount: number | null
          redeemed_count: number | null
          responses: number | null
          survey_id: string | null
          void_count: number | null
        }
        Relationships: [
          {
            foreignKeyName: "survey_project_id_fkey"
            columns: ["project_id"]
            isOneToOne: false
            referencedRelation: "project"
            referencedColumns: ["id"]
          },
        ]
      }
      v_village_data_quality: {
        Row: {
          cycles: number | null
          cycles_with_estimate: number | null
          farms: number | null
          farms_with_gps: number | null
          persons: number | null
          persons_verified: number | null
          village_id: string | null
        }
        Insert: {
          cycles?: never
          cycles_with_estimate?: never
          farms?: never
          farms_with_gps?: never
          persons?: never
          persons_verified?: never
          village_id?: string | null
        }
        Update: {
          cycles?: never
          cycles_with_estimate?: never
          farms?: never
          farms_with_gps?: never
          persons?: never
          persons_verified?: never
          village_id?: string | null
        }
        Relationships: []
      }
      v_village_energy: {
        Row: {
          approved_assets: number | null
          approved_kw_raw: number | null
          approved_kwh_per_week: number | null
          approved_peak_kw: number | null
          capacity_basis: Database["public"]["Enums"]["capacity_basis"] | null
          capacity_kw: number | null
          headroom_kw: number | null
          prospective_kw_raw: number | null
          prospective_peak_kw: number | null
          simultaneity_factor: number | null
          village_id: string | null
        }
        Relationships: []
      }
      v_village_production: {
        Row: {
          actual_kg: number | null
          crop_id: string | null
          cycle_area_ha: number | null
          cycle_count: number | null
          expected_kg: number | null
          plot_count: number | null
          tree_count: number | null
          verified_cycles: number | null
          village_id: string | null
          window_month: string | null
        }
        Relationships: [
          {
            foreignKeyName: "crop_cycle_crop_id_fkey"
            columns: ["crop_id"]
            isOneToOne: false
            referencedRelation: "crop"
            referencedColumns: ["id"]
          },
        ]
      }
      v_village_pue_pipeline: {
        Row: {
          currency: string | null
          indicative_rent_per_day: number | null
          indicative_value: number | null
          request_count: number | null
          status: Database["public"]["Enums"]["pue_status"] | null
          village_id: string | null
        }
        Relationships: [
          {
            foreignKeyName: "pue_request_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_data_quality"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "pue_request_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "v_village_energy"
            referencedColumns: ["village_id"]
          },
          {
            foreignKeyName: "pue_request_village_id_fkey"
            columns: ["village_id"]
            isOneToOne: false
            referencedRelation: "village"
            referencedColumns: ["id"]
          },
        ]
      }
      v_village_supply: {
        Row: {
          available_kg: number | null
          committed_kg: number | null
          crop_id: string | null
          cycle_count: number | null
          expected_kg: number | null
          verified_cycles: number | null
          village_id: string | null
          window_end: string | null
          window_month: string | null
          window_start: string | null
        }
        Relationships: [
          {
            foreignKeyName: "crop_cycle_crop_id_fkey"
            columns: ["crop_id"]
            isOneToOne: false
            referencedRelation: "crop"
            referencedColumns: ["id"]
          },
        ]
      }
    }
    Functions: {
      app_actor_display_name: { Args: never; Returns: string }
      app_actor_id: { Args: never; Returns: string }
      app_actor_names: {
        Args: { p_ids: string[] }
        Returns: {
          display_name: string
          id: string
        }[]
      }
      app_actor_role: { Args: never; Returns: string }
      app_admins_project: { Args: { p: string }; Returns: boolean }
      app_create_village: { Args: { payload: Json }; Returns: Json }
      app_farmer_login_issue: { Args: { p_person_id: string }; Returns: Json }
      app_farms: { Args: never; Returns: string[] }
      app_has_role: {
        Args: { r: Database["public"]["Enums"]["app_role"] }
        Returns: boolean
      }
      app_household_persons: { Args: never; Returns: string[] }
      app_households: { Args: never; Returns: string[] }
      app_is_staff: { Args: never; Returns: boolean }
      app_login_email_for_phone: { Args: { p_phone: string }; Returns: string }
      app_manage_opportunity: {
        Args: { p_opportunity: string }
        Returns: boolean
      }
      app_manage_village: { Args: { p_village: string }; Returns: boolean }
      app_manages_project: { Args: { p: string }; Returns: boolean }
      app_must_change_password: { Args: never; Returns: boolean }
      app_normalize_phone: { Args: { p_phone: string }; Returns: string }
      app_password_changed: { Args: never; Returns: undefined }
      app_person_id: { Args: never; Returns: string }
      app_person_login_history: {
        Args: { p_person_id: string }
        Returns: {
          issued_at: string
          issued_by_name: string
          kind: string
          must_change_password: boolean
        }[]
      }
      app_plot_document_village: {
        Args: { object_name: string }
        Returns: string
      }
      app_projects: { Args: never; Returns: string[] }
      app_random_crockford: { Args: { p_length: number }; Returns: string }
      app_redemption_log: {
        Args: { p_from: string; p_to: string }
        Returns: {
          amount: number
          audit_required: boolean
          currency: string
          household_label: string
          id_type_seen: Database["public"]["Enums"]["id_document_type"]
          redeemed_at: string
          redeemed_by: string
          redeemed_by_name: string
          survey_title_en: string
          survey_title_sw: string
          village_name: string
          voucher_id: string
        }[]
      }
      app_redemption_totals: {
        Args: { p_from: string; p_to: string }
        Returns: {
          amount: number
          currency: string
          day: string
          redeemed_by: string
          redeemed_by_name: string
          vouchers: number
        }[]
      }
      app_register_farmer: { Args: { payload: Json }; Returns: Json }
      app_staff_households: { Args: never; Returns: string[] }
      app_staff_opportunities: { Args: never; Returns: string[] }
      app_staff_villages: { Args: never; Returns: string[] }
      app_supersede_harvest: {
        Args: {
          p_confidence?: Database["public"]["Enums"]["confidence_level"]
          p_cycle: string
          p_kind: Database["public"]["Enums"]["harvest_kind"]
          p_quantity_kg: number
          p_reported_for?: string
          p_source: Database["public"]["Enums"]["source_type"]
        }
        Returns: string
      }
      app_supplied_opportunities: { Args: never; Returns: string[] }
      app_survey_eligibility: {
        Args: never
        Returns: {
          eligible: boolean
          reason: string
          response_id: string
          survey_id: string
          voucher_id: string
        }[]
      }
      app_survey_household_count: {
        Args: { p_survey: string }
        Returns: number
      }
      app_survey_submit: {
        Args: { p_answers: Json; p_client_ref: string; p_survey_id: string }
        Returns: Json
      }
      app_survey_vouchers: {
        Args: { p_survey_id: string }
        Returns: {
          amount: number
          audit_required: boolean
          currency: string
          expired: boolean
          expires_at: string
          household_label: string
          id_type_seen: Database["public"]["Enums"]["id_document_type"]
          issued_at: string
          redeemed_at: string
          redeemed_by_name: string
          respondent_name: string
          status: Database["public"]["Enums"]["voucher_status"]
          submitted_at: string
          void_reason: string
          voucher_id: string
        }[]
      }
      app_update_observed_record: {
        Args: { p_id: string; p_payload: Json; p_table: string }
        Returns: Json
      }
      app_user_role_label: { Args: { p_user: string }; Returns: string }
      app_verify: {
        Args: { p_id: string; p_table: string }
        Returns: undefined
      }
      app_villages: { Args: never; Returns: string[] }
      app_voucher_code: { Args: { p_voucher_id: string }; Returns: string }
      app_voucher_lookup: { Args: { p_code: string }; Returns: Json }
      app_voucher_normalize: { Args: { p_code: string }; Returns: string }
      app_voucher_redeem: {
        Args: {
          p_code: string
          p_id_type: Database["public"]["Enums"]["id_document_type"]
          p_name_confirmed: boolean
        }
        Returns: Json
      }
      app_voucher_resolve: { Args: { p_code: string }; Returns: string }
      app_voucher_timeline: {
        Args: { p_voucher_id: string }
        Returns: {
          actor_name: string
          actor_role: string
          detail: Json
          kind: string
          occurred_at: string
        }[]
      }
      app_voucher_void: {
        Args: { p_reason: string; p_voucher_id: string }
        Returns: undefined
      }
      assert_eq: {
        Args: { actual: number; expected: number; label: string }
        Returns: undefined
      }
      assert_raises: {
        Args: { label: string; stmt: string }
        Returns: undefined
      }
      survey_block_reason: { Args: { p_survey_id: string }; Returns: string }
      voucher_block_reason: {
        Args: {
          p_voucher: Database["public"]["Tables"]["survey_voucher"]["Row"]
        }
        Returns: string
      }
    }
    Enums: {
      acquisition_mode: "rent" | "buy"
      app_role: "farmer" | "field_officer" | "ops" | "admin"
      area_unit: "hectare" | "acre"
      bug_status: "open" | "in_progress" | "fixed" | "wont_fix"
      buyer_channel: "direct" | "afm" | "other"
      capacity_basis: "planned" | "nameplate"
      confidence_level: "low" | "medium" | "high"
      crop_cycle_status: "planned" | "growing" | "harvested" | "abandoned"
      crop_measure: "area" | "tree_count" | "unit_count"
      demand_status: "open" | "matched" | "closed" | "cancelled"
      harvest_kind: "expected" | "actual"
      id_document_type: "nida" | "voter" | "driving_licence" | "village_letter"
      opportunity_status:
        | "proposed"
        | "shared"
        | "accepted"
        | "declined"
        | "lapsed"
      project_status: "setup" | "active" | "paused" | "closed"
      pue_status:
        | "draft"
        | "submitted"
        | "under_review"
        | "approved"
        | "rejected"
        | "withdrawn"
      source_type:
        | "farmer_reported"
        | "field_verified"
        | "transaction_derived"
        | "sensor_derived"
        | "model_estimated"
      survey_question_kind:
        | "single_choice"
        | "multi_choice"
        | "number"
        | "text"
        | "yes_no"
      survey_status: "draft" | "live" | "closed"
      verification_status: "unverified" | "pending" | "verified" | "disputed"
      voucher_event_kind:
        | "issued"
        | "scanned"
        | "refused"
        | "redeemed"
        | "voided"
      voucher_status: "issued" | "redeemed" | "void"
    }
    CompositeTypes: {
      [_ in never]: never
    }
  }
}

type DatabaseWithoutInternals = Omit<Database, "__InternalSupabase">

type DefaultSchema = DatabaseWithoutInternals[Extract<keyof Database, "public">]

export type Tables<
  DefaultSchemaTableNameOrOptions extends
    | keyof (DefaultSchema["Tables"] & DefaultSchema["Views"])
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends (DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
        DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])
    : never) = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
      DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])[TableName] extends {
      Row: infer R
    }
    ? R
    : never
  : DefaultSchemaTableNameOrOptions extends keyof (DefaultSchema["Tables"] &
        DefaultSchema["Views"])
    ? (DefaultSchema["Tables"] &
        DefaultSchema["Views"])[DefaultSchemaTableNameOrOptions] extends {
        Row: infer R
      }
      ? R
      : never
    : never

export type TablesInsert<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends (DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never) = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Insert: infer I
    }
    ? I
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Insert: infer I
      }
      ? I
      : never
    : never

export type TablesUpdate<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends (DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never) = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Update: infer U
    }
    ? U
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Update: infer U
      }
      ? U
      : never
    : never

export type Enums<
  DefaultSchemaEnumNameOrOptions extends
    | keyof DefaultSchema["Enums"]
    | { schema: keyof DatabaseWithoutInternals },
  EnumName extends (DefaultSchemaEnumNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"]
    : never) = never,
> = DefaultSchemaEnumNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"][EnumName]
  : DefaultSchemaEnumNameOrOptions extends keyof DefaultSchema["Enums"]
    ? DefaultSchema["Enums"][DefaultSchemaEnumNameOrOptions]
    : never

export type CompositeTypes<
  PublicCompositeTypeNameOrOptions extends
    | keyof DefaultSchema["CompositeTypes"]
    | { schema: keyof DatabaseWithoutInternals },
  CompositeTypeName extends (PublicCompositeTypeNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"]
    : never) = never,
> = PublicCompositeTypeNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"][CompositeTypeName]
  : PublicCompositeTypeNameOrOptions extends keyof DefaultSchema["CompositeTypes"]
    ? DefaultSchema["CompositeTypes"][PublicCompositeTypeNameOrOptions]
    : never

export const Constants = {
  graphql_public: {
    Enums: {},
  },
  public: {
    Enums: {
      acquisition_mode: ["rent", "buy"],
      app_role: ["farmer", "field_officer", "ops", "admin"],
      area_unit: ["hectare", "acre"],
      bug_status: ["open", "in_progress", "fixed", "wont_fix"],
      buyer_channel: ["direct", "afm", "other"],
      capacity_basis: ["planned", "nameplate"],
      confidence_level: ["low", "medium", "high"],
      crop_cycle_status: ["planned", "growing", "harvested", "abandoned"],
      crop_measure: ["area", "tree_count", "unit_count"],
      demand_status: ["open", "matched", "closed", "cancelled"],
      harvest_kind: ["expected", "actual"],
      id_document_type: ["nida", "voter", "driving_licence", "village_letter"],
      opportunity_status: [
        "proposed",
        "shared",
        "accepted",
        "declined",
        "lapsed",
      ],
      project_status: ["setup", "active", "paused", "closed"],
      pue_status: [
        "draft",
        "submitted",
        "under_review",
        "approved",
        "rejected",
        "withdrawn",
      ],
      source_type: [
        "farmer_reported",
        "field_verified",
        "transaction_derived",
        "sensor_derived",
        "model_estimated",
      ],
      survey_question_kind: [
        "single_choice",
        "multi_choice",
        "number",
        "text",
        "yes_no",
      ],
      survey_status: ["draft", "live", "closed"],
      verification_status: ["unverified", "pending", "verified", "disputed"],
      voucher_event_kind: [
        "issued",
        "scanned",
        "refused",
        "redeemed",
        "voided",
      ],
      voucher_status: ["issued", "redeemed", "void"],
    },
  },
} as const
