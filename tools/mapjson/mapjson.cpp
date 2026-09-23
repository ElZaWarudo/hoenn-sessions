// mapjson.cpp

#include <iostream>
using std::cout; using std::endl;

#include <string>
using std::string;

#include <vector>
using std::vector;

#include <algorithm>
using std::sort; using std::find;

#include <map>
using std::map;

#include <set>
using std::set;

#include <fstream>
using std::ofstream; using std::ifstream;

#include <sstream>
using std::ostringstream;

#include <limits>
using std::numeric_limits;

#include "json11.h"
using json11::Json;

#include <regex>

#include "mapjson.h"

#include <filesystem>

#define TRUE  1
#define FALSE 0

enum
{
    MAP_ENGINE_REGION_HOENN_VALUE = 0,
    MAP_ENGINE_REGION_KANTO_VALUE = 1,
    MAP_ENGINE_REGION_JOHTO_VALUE = 2,
};

static_assert(MAP_ENGINE_REGION_HOENN_VALUE == 0, "Hoenn map header byte");
static_assert(MAP_ENGINE_REGION_KANTO_VALUE == 1, "Kanto map header byte");
static_assert(MAP_ENGINE_REGION_JOHTO_VALUE == 2, "Johto map header byte");

// expansion headers
#include "../../include/config/frlg.h"

string version;
// System directory separator
string sep;

string read_text_file(string filepath) {
    ifstream in_file(filepath, std::ifstream::binary);

    if (!in_file.is_open())
        FATAL_ERROR("Cannot open file %s for reading.\n", filepath.c_str());

    string text;

    in_file.seekg(0, std::ios::end);
    text.resize(in_file.tellg());

    in_file.seekg(0, std::ios::beg);
    in_file.read(&text[0], text.size());

    in_file.close();

    return text;
}

void write_text_file(string filepath, string text) {
    ofstream out_file(filepath, std::ofstream::binary);

    if (!out_file.is_open())
        FATAL_ERROR("Cannot open file %s for writing.\n", filepath.c_str());

    out_file << text;

    out_file.close();
}


string json_to_string(const Json &data, const string &field = "", bool silent = false) {
    const Json value = !field.empty() ? data[field] : data;
    string output = "";
    switch (value.type()) {
        case Json::Type::STRING:
            output = value.string_value();
            break;
        case Json::Type::NUMBER:
            output = std::to_string(value.int_value());
            break;
        case Json::Type::BOOL:
            output = value.bool_value() ? "TRUE" : "FALSE";
            break;
        case Json::Type::NUL:
            output = "";
            break;
        default:{
            if (!silent) {
                string s = !field.empty() ? ("Value for '" + field + "'") : "JSON field";
                FATAL_ERROR("%s is unexpected type; expected string, number, or bool.\n", s.c_str());
            }
        }
    }

    if (!silent && output.empty()) {
        string s = !field.empty() ? ("Value for '" + field + "'") : "JSON field";
        FATAL_ERROR("%s cannot be empty.\n", s.c_str());
    }

    return output;
}

int get_map_engine_region_value(const Json &map_data) {
    string section = json_to_string(map_data, "region_map_section");
    bool has_region = map_data.object_items().find("region") != map_data.object_items().end();
    string region = has_region ? json_to_string(map_data, "region") : "REGION_HOENN";

    /* A missing region must not silently assign a Johto section to Hoenn. */
    bool johto_section = section == "MAPSEC_NEW_BARK_TOWN"
                      || section.rfind("MAPSEC_JOHTO_", 0) == 0;
    bool kanto_border_section = section == "MAPSEC_JOHTO_ROUTE_26"
                             || section == "MAPSEC_JOHTO_ROUTE_27"
                             || section == "MAPSEC_JOHTO_ROUTE_28";
    if ((kanto_border_section && region != "REGION_KANTO")
     || (!kanto_border_section && (region == "REGION_JOHTO") != johto_section))
        FATAL_ERROR("Map engine region '%s' contradicts section '%s'.\n", region.c_str(), section.c_str());

    /* An absent region is the only form that inherits the original Emerald
     * default. The emitted byte intentionally uses stable assembly values;
     * data/maps.s cannot import the C Region enum header. */
    if (!has_region)
        return MAP_ENGINE_REGION_HOENN_VALUE;

    if (region == "REGION_HOENN")
        return MAP_ENGINE_REGION_HOENN_VALUE;
    if (region == "REGION_KANTO")
        return MAP_ENGINE_REGION_KANTO_VALUE;
    if (region == "REGION_JOHTO")
        return MAP_ENGINE_REGION_JOHTO_VALUE;
    FATAL_ERROR("Unknown or unsupported map engine region '%s'.\n", region.c_str());
}

void validate_map_section_width(const string &section) {
    static const std::map<string, size_t> section_ids = [] {
        string error;
        Json data = Json::parse(read_text_file("src/data/region_map/region_map_sections.json"), error);
        if (data == Json())
            FATAL_ERROR("Cannot parse map sections: %s\n", error.c_str());
        std::map<string, size_t> ids;
        const auto sections = data["map_sections"].array_items();
        for (size_t index = 0; index < sections.size(); ++index) {
            string id = json_to_string(sections[index], "id");
            if (!ids.emplace(id, index).second)
                FATAL_ERROR("Duplicate map section '%s'.\n", id.c_str());
        }
        ids.emplace("MAPSEC_NONE", sections.size());
        return ids;
    }();
    auto found = section_ids.find(section);
    if (found == section_ids.end())
        FATAL_ERROR("Unknown map section '%s'.\n", section.c_str());
    if (found->second > 65535)
        FATAL_ERROR("Map section '%s' has ID %zu, but map headers hold two bytes.\n", section.c_str(), found->second);
}

string get_generated_warning(const string &filename, bool isAsm) {
    string comment = isAsm ? "@" : "//";

    ostringstream warning;
    warning << comment << "\n"
            << comment << " DO NOT MODIFY THIS FILE! It is auto-generated from " << filename << "\n"
            << comment << "\n\n";
    return warning.str();
}

string get_include_guard_start(const string &name) {
    ostringstream guard;
    guard << "#ifndef GUARD_" << name << "_H\n"
          << "#define GUARD_" << name << "_H\n\n";
    return guard.str();
}

string get_include_guard_end(const string &name) {
    ostringstream guard;
    guard << "#endif // GUARD_" << name << "_H\n";
    return guard.str();
}

string generate_map_header_text(Json map_data, Json layouts_data) {
    string map_layout_id = json_to_string(map_data, "layout");

    vector<Json> matched;

    for (auto &layout : layouts_data["layouts"].array_items()) {
        if (map_layout_id == json_to_string(layout, "id", true))
            matched.push_back(layout);
    }

    if (matched.size() != 1)
        FATAL_ERROR("Failed to find matching layout for %s.\n", map_layout_id.c_str());

    Json layout = matched[0];

    ostringstream text;

    string mapName = json_to_string(map_data, "name");
    int engine_region = get_map_engine_region_value(map_data);
    validate_map_section_width(json_to_string(map_data, "region_map_section"));
    text << get_generated_warning("data/maps/" + mapName + "/map.json", true);

    text << mapName << ":\n"
         << "\t.4byte " << json_to_string(layout, "name") << "\n";

    if (map_data.object_items().find("shared_events_map") != map_data.object_items().end())
        text << "\t.4byte " << json_to_string(map_data, "shared_events_map") << "_MapEvents\n";
    else
        text << "\t.4byte " << mapName << "_MapEvents\n";

    if (map_data.object_items().find("shared_scripts_map") != map_data.object_items().end())
        text << "\t.4byte " << json_to_string(map_data, "shared_scripts_map") << "_MapScripts\n";
    else
        text << "\t.4byte " << mapName << "_MapScripts\n";

    if (map_data.object_items().find("connections") != map_data.object_items().end()
     && map_data["connections"].array_items().size() > 0 && json_to_string(map_data, "connections_no_include", true) != "TRUE")
        text << "\t.4byte " << mapName << "_MapConnections\n";
    else
        text << "\t.4byte NULL\n";

    text << "\t.2byte " << json_to_string(map_data, "music") << "\n"
         << "\t.2byte " << json_to_string(layout, "id") << "\n"
         << "\t.2byte " << json_to_string(map_data, "region_map_section") << "\n"
         << "\t.byte "  << json_to_string(map_data, "requires_flash") << "\n"
         << "\t.byte "  << json_to_string(map_data, "weather") << "\n"
         << "\t.byte "  << json_to_string(map_data, "map_type") << "\n";

    string floor_number = json_to_string(map_data, "floor_number", true);
    if (floor_number.empty())
        text << "\t.byte 0\n";
    else
        text << "\t.byte " << floor_number << "\n";

    text << "\t.byte " << engine_region << "\n";

    if (version == "ruby")
        text << "\t.byte " << json_to_string(map_data, "show_map_name") << "\n";
    else if (version == "emerald" || version == "firered")
        text << "\tmap_header_flags "
             << "allow_cycling=" << json_to_string(map_data, "allow_cycling") << ", "
             << "allow_escaping=" << json_to_string(map_data, "allow_escaping") << ", "
             << "allow_running=" << json_to_string(map_data, "allow_running") << ", "
             << "show_map_name=" << json_to_string(map_data, "show_map_name") << "\n";

     text << "\t.byte " << json_to_string(map_data, "battle_scene") << "\n"
          << "\t.balign 4, 0\n\n";

    return text.str();
}

vector<string> get_existing_maps() {
    vector<string> v = {};
    string map_constants = read_text_file("include/constants/map_groups.h");

    std::regex map_regex("(MAP_\\w+)\\s+=\\s+\\(\\d+");

    for (std::smatch sm; regex_search(map_constants, sm, map_regex);)
    {
        v.push_back(sm[1]);
        map_constants = sm.suffix();
    }
    return v;
}

string generate_map_connections_text(Json map_data) {
    if (map_data["connections"] == Json())
        return string("\n");

    string mapName = json_to_string(map_data, "name");

    vector<string> existing_maps = get_existing_maps();
    ostringstream text;
    text << get_generated_warning("data/maps/" + mapName + "/map.json", true);
    text << mapName << "_MapConnectionsList:\n";

    for (auto &connection : map_data["connections"].array_items()) {
        auto it = find(existing_maps.begin(), existing_maps.end(), json_to_string(connection, "map"));
        if (it == existing_maps.end())
            continue;
        text << "\tconnection "
             << json_to_string(connection, "direction") << ", "
             << json_to_string(connection, "offset") << ", "
             << json_to_string(connection, "map") << "\n";
    }

    text << "\n" << mapName << "_MapConnections:\n"
         << "\t.4byte " << map_data["connections"].array_items().size() << "\n"
         << "\t.4byte " << mapName << "_MapConnectionsList\n\n";

    return text.str();
}

string generate_map_events_text(Json map_data) {
    if (map_data.object_items().find("shared_events_map") != map_data.object_items().end())
        return string("\n");

    string mapName = json_to_string(map_data, "name");

    ostringstream text;
    text << get_generated_warning("data/maps/" + mapName + "/map.json", true);
    text << "\t.align 2\n\n";

    string objects_label, warps_label, coords_label, bgs_label;

    if (map_data["object_events"].array_items().size() > 0) {
        objects_label = mapName + "_ObjectEvents";
        text << objects_label << ":\n";
        for (unsigned int i = 0; i < map_data["object_events"].array_items().size(); i++) {
            auto obj_event = map_data["object_events"].array_items()[i];
            string type = json_to_string(obj_event, "type", true);

            // If no type field is present, assume it's a regular object event.
            if (type == "" || type == "object") {
                text << "\tobject_event " << i + 1 << ", "
                     << json_to_string(obj_event, "graphics_id") << ", "
                     << json_to_string(obj_event, "x") << ", "
                     << json_to_string(obj_event, "y") << ", "
                     << json_to_string(obj_event, "elevation") << ", "
                     << json_to_string(obj_event, "movement_type") << ", "
                     << json_to_string(obj_event, "movement_range_x") << ", "
                     << json_to_string(obj_event, "movement_range_y") << ", "
                     << json_to_string(obj_event, "trainer_type") << ", "
                     << json_to_string(obj_event, "trainer_sight_or_berry_tree_id") << ", "
                     << json_to_string(obj_event, "script") << ", "
                     << json_to_string(obj_event, "flag") << "\n";
            } else if (type == "clone") {
                text << "\tclone_event " << i + 1 << ", "
                     << json_to_string(obj_event, "graphics_id") << ", "
                     << json_to_string(obj_event, "x") << ", "
                     << json_to_string(obj_event, "y") << ", "
                     << json_to_string(obj_event, "target_local_id") << ", "
                     << json_to_string(obj_event, "target_map") << "\n";
            } else {
                FATAL_ERROR("Unknown object event type '%s'. Expected 'object' or 'clone'.\n", type.c_str());
            }
        }
        text << "\n";
    } else {
        objects_label = "NULL";
    }

    if (map_data["warp_events"].array_items().size() > 0) {
        warps_label = mapName + "_MapWarps";
        text << warps_label << ":\n";
        for (auto &warp_event : map_data["warp_events"].array_items()) {
            text << "\twarp_def "
                 << json_to_string(warp_event, "x") << ", "
                 << json_to_string(warp_event, "y") << ", "
                 << json_to_string(warp_event, "elevation") << ", "
                 << json_to_string(warp_event, "dest_warp_id") << ", "
                 << json_to_string(warp_event, "dest_map") << "\n";
        }
        text << "\n";
    } else {
        warps_label = "NULL";
    }

    if (map_data["coord_events"].array_items().size() > 0) {
        coords_label = mapName + "_MapCoordEvents";
        text << coords_label << ":\n";
        for (auto &coord_event : map_data["coord_events"].array_items()) {
            string type = json_to_string(coord_event, "type");
            if (type == "trigger") {
                text << "\tcoord_event "
                     << json_to_string(coord_event, "x") << ", "
                     << json_to_string(coord_event, "y") << ", "
                     << json_to_string(coord_event, "elevation") << ", "
                     << json_to_string(coord_event, "var") << ", "
                     << json_to_string(coord_event, "var_value") << ", "
                     << json_to_string(coord_event, "script") << "\n";
            }
            else if (type == "weather") {
                text << "\tcoord_weather_event "
                     << json_to_string(coord_event, "x") << ", "
                     << json_to_string(coord_event, "y") << ", "
                     << json_to_string(coord_event, "elevation") << ", "
                     << json_to_string(coord_event, "weather") << "\n";
            } else {
                FATAL_ERROR("Unknown coord event type '%s'. Expected 'trigger' or 'weather'.\n", type.c_str());
            }
        }
        text << "\n";
    } else {
        coords_label = "NULL";
    }

    if (map_data["bg_events"].array_items().size() > 0) {
        bgs_label = mapName + "_MapBGEvents";
        text << bgs_label << ":\n";
        for (auto &bg_event : map_data["bg_events"].array_items()) {
            string type = json_to_string(bg_event, "type");
            if (type == "sign") {
                text << "\tbg_sign_event "
                     << json_to_string(bg_event, "x") << ", "
                     << json_to_string(bg_event, "y") << ", "
                     << json_to_string(bg_event, "elevation") << ", "
                     << json_to_string(bg_event, "player_facing_dir") << ", "
                     << json_to_string(bg_event, "script") << "\n";
            }
            else if (type == "hidden_item") {
                string quantity = json_to_string(bg_event, "quantity", true);
                if (quantity.empty()) {
                    quantity = "1";
                }
                string underfoot = json_to_string(bg_event, "underfoot", true);
                if (underfoot.empty()) {
                    underfoot = "FALSE";
                }
                text << "\tbg_hidden_item_event "
                     << json_to_string(bg_event, "x") << ", "
                     << json_to_string(bg_event, "y") << ", "
                     << json_to_string(bg_event, "elevation") << ", "
                     << json_to_string(bg_event, "item") << ", "
                     << json_to_string(bg_event, "flag") << ", "
                     << quantity << ", "
                     << underfoot << "\n";
            }
            else if (type == "secret_base") {
                text << "\tbg_secret_base_event "
                     << json_to_string(bg_event, "x") << ", "
                     << json_to_string(bg_event, "y") << ", "
                     << json_to_string(bg_event, "elevation") << ", "
                     << json_to_string(bg_event, "secret_base_id") << "\n";
            } else {
                FATAL_ERROR("Unknown bg event type '%s'. Expected 'sign', 'hidden_item', or 'secret_base'.\n", type.c_str());
            }
        }
        text << "\n";
    } else {
        bgs_label = "NULL";
    }

    text << mapName << "_MapEvents::\n"
         << "\tmap_events " << objects_label << ", " << warps_label << ", "
         << coords_label << ", " << bgs_label << "\n\n";

    return text.str();
}

string strip_trailing_separator(string filename) {
    if(filename.back() == '/' || filename.back() == '\\')
        filename.pop_back();

    return filename;
}
void infer_separator(string filename) {
    size_t dir_pos = filename.find_last_of("/\\");
    sep = filename[dir_pos];
}
string file_parent(string filename){
    size_t dir_pos = filename.find_last_of("/\\");
    return filename.substr(0, dir_pos + 1);
}

void process_map(string map_filepath, string layouts_filepath, string output_dir) {
    string mapdata_err, layouts_err;

    string mapdata_json_text = read_text_file(map_filepath);
    string layouts_json_text = read_text_file(layouts_filepath);

    Json map_data = Json::parse(mapdata_json_text, mapdata_err);
    if (map_data == Json())
        FATAL_ERROR("%s\n", mapdata_err.c_str());

    Json layouts_data = Json::parse(layouts_json_text, layouts_err);
    if (layouts_data == Json())
        FATAL_ERROR("%s\n", layouts_err.c_str());

    string header_text = generate_map_header_text(map_data, layouts_data);
    string events_text = generate_map_events_text(map_data);
    string connections_text = generate_map_connections_text(map_data);

    string out_dir = strip_trailing_separator(output_dir).append(sep);
    write_text_file(out_dir + "header.inc", header_text);
    write_text_file(out_dir + "events.inc", events_text);
    write_text_file(out_dir + "connections.inc", connections_text);
}

void process_event_constants(const vector<string> &map_filepaths, string output_ids_file) {
    string warning = get_generated_warning("data/maps/*/map.json", false);

    string guard_name = "CONSTANTS_MAP_EVENT_IDS";
    ostringstream ids_file_text;
    ids_file_text << get_include_guard_start(guard_name) << warning;

    for (const string &filepath : map_filepaths) {
        string err;
        string map_json_text = read_text_file(filepath);
        Json map_data = Json::parse(map_json_text, err);
        if (map_data == Json())
            FATAL_ERROR("Failed to read '%s' while generating map event constants: %s\n", filepath.c_str(), err.c_str());

        string map_id = json_to_string(map_data, "id");

        // Get IDs from the object/clone events.
        ostringstream map_ids_text;
        auto obj_events = map_data["object_events"].array_items();
        for (unsigned int i = 0; i < obj_events.size(); i++) {
            auto obj_event = obj_events[i];
            if (obj_event.object_items().find("local_id") != obj_event.object_items().end())
                map_ids_text << "#define " << json_to_string(obj_event, "local_id") << " " << i + 1 << "\n";
        }
        // Get IDs from the warp events.
        auto warp_events = map_data["warp_events"].array_items();
        for (unsigned int i = 0; i < warp_events.size(); i++) {
            auto warp_event = warp_events[i];
            if (warp_event.object_items().find("warp_id") != warp_event.object_items().end())
                map_ids_text << "#define " << json_to_string(warp_event, "warp_id") << " " << i << "\n";
        }
        // Only output if we found any IDs
        string temp = map_ids_text.str();
        if (!temp.empty()) {
            ids_file_text << "// " << map_id << "\n" << temp << "\n";
        }
    }

    ids_file_text << get_include_guard_end(guard_name);
    write_text_file(output_ids_file, ids_file_text.str());
}

// Stable map/layout IDs are common to every ROM. Membership affects linked content only.
struct RomWorldRegistry {
    map<string, int> bits;
    int all_bits;
    int default_bit;
};

int registry_integer(const Json &value, const string &field, int maximum) {
    if (!value.is_number() || value.number_value() < 1 || value.number_value() > maximum
     || value.number_value() != static_cast<int>(value.number_value()))
        FATAL_ERROR("ROM world registry: %s must be an integer from 1 through %d\n", field.c_str(), maximum);
    return value.int_value();
}

RomWorldRegistry load_rom_world_registry() {
    const string path = "data/rom_worlds.json";
    string error;
    const Json data = Json::parse(read_text_file(path), error);
    RomWorldRegistry registry = {{}, 0, 0};
    set<int> ids;
    set<int> bits;
    set<string> build_names;
    set<string> game_codes;

    if (!error.empty() || !data.is_object() || data["schema_version"] != 1
     || !data["worlds"].is_array() || data["worlds"].array_items().empty()
     || !data["default_world"].is_string())
        FATAL_ERROR("%s: invalid ROM world registry\n", path.c_str());

    for (const Json &world : data["worlds"].array_items()) {
        if (!world.is_object() || !world["name"].is_string())
            FATAL_ERROR("%s: invalid ROM world entry\n", path.c_str());
        const string name = world["name"].string_value();
        if (!std::regex_match(name, std::regex("[a-z][a-z0-9_]*")) || name == "shared")
            FATAL_ERROR("%s: invalid ROM world name %s\n", path.c_str(), name.c_str());
        const int id = registry_integer(world["world_id"], name + " world_id", 65535);
        const int bit = registry_integer(world["build_bit"], name + " build_bit", 1 << 30);
        if ((bit & (bit - 1)) != 0 || !ids.insert(id).second || !bits.insert(bit).second
         || !registry.bits.emplace(name, bit).second)
            FATAL_ERROR("%s: duplicate or invalid ROM world identity %s\n", path.c_str(), name.c_str());
        const bool is_default = name == data["default_world"].string_value();
        const Json game_version = world["game_version"];
        const Json map_version = world["map_version"];
        const Json build_name = world["build_name"];
        const Json title = world["title"];
        const Json game_code = world["game_code"];
        if ((!game_version.is_null() && (!game_version.is_string()
             || (game_version.string_value() != "EMERALD" && game_version.string_value() != "FIRERED"
              && game_version.string_value() != "LEAFGREEN")))
         || (!map_version.is_null() && (!map_version.is_string()
             || (map_version.string_value() != "emerald" && map_version.string_value() != "firered")))
         || (!build_name.is_null() && (!build_name.is_string()
             || !std::regex_match(build_name.string_value(), std::regex("[a-z0-9][a-z0-9-]*"))))
         || (!title.is_null() && (!title.is_string()
             || !std::regex_match(title.string_value(), std::regex("[A-Z0-9 ]{1,12}"))))
         || (!game_code.is_null() && (!game_code.is_string()
             || !std::regex_match(game_code.string_value(), std::regex("[A-Z0-9]{4}"))))
         || (!is_default && (game_version.is_null() || map_version.is_null()
             || build_name.is_null() || title.is_null() || game_code.is_null()))
         || (is_default && (!game_version.is_null() || !map_version.is_null()
             || !build_name.is_null() || !title.is_null() || !game_code.is_null())))
            FATAL_ERROR("%s: invalid build metadata for ROM world %s\n", path.c_str(), name.c_str());
        if ((!build_name.is_null()
             && (!build_names.insert(build_name.string_value()).second
              || (!is_default && (build_name.string_value() == "emerald"
                  || build_name.string_value() == "firered" || build_name.string_value() == "leafgreen"))))
         || (!game_code.is_null()
             && (!game_codes.insert(game_code.string_value()).second
              || (!is_default && (game_code.string_value() == "BPEE"
                  || game_code.string_value() == "BPRE" || game_code.string_value() == "BPGE")))))
            FATAL_ERROR("%s: duplicate ROM artifact identity for world %s\n", path.c_str(), name.c_str());
        registry.all_bits |= bit;
    }

    const auto found = registry.bits.find(data["default_world"].string_value());
    if (found == registry.bits.end() || found->second != 1)
        FATAL_ERROR("%s: default_world must have build_bit 1\n", path.c_str());
    registry.default_bit = found->second;
    return registry;
}

int rom_world_mask(const Json &data, const string &owner) {
    static const RomWorldRegistry registry = load_rom_world_registry();
    auto field = data.object_items().find("rom_world");
    if (field == data.object_items().end())
        return registry.default_bit; // Existing maps/layouts remain in the default world.
    if (field->second.is_string()) {
        const string world = field->second.string_value();
        if (world == "shared") return registry.all_bits;
        const auto found = registry.bits.find(world);
        if (found != registry.bits.end()) return found->second;
    } else if (field->second.is_array() && !field->second.array_items().empty()) {
        int mask = 0;
        for (const Json &member : field->second.array_items()) {
            if (!member.is_string() || registry.bits.count(member.string_value()) == 0
             || (mask & registry.bits.at(member.string_value())) != 0)
                FATAL_ERROR("%s: rom_world array has an invalid or duplicate world\n", owner.c_str());
            mask |= registry.bits.at(member.string_value());
        }
        return mask;
    }
    FATAL_ERROR("%s: rom_world must name registered worlds or shared\n", owner.c_str());
}

void begin_rom_world(ostringstream &text, int mask) {
    text << "\t.if (ROM_WORLD & " << mask << ")\n";
}

string generate_groups_text(Json groups_data, vector<string> &invalid_maps, const map<string, int> &worlds) {
    ostringstream text;

    text << get_generated_warning("data/maps/map_groups.json", true);

    vector<string> valid_groups;
    for (auto &key : groups_data["group_order"].array_items()) {
        string group = json_to_string(key);
        auto maps = groups_data[group].array_items();
        if (!maps.empty()) {
            text << group << "::\n";
            for (const Json &map_name : maps) {
                const string name = json_to_string(map_name);
                if (find(invalid_maps.begin(), invalid_maps.end(), name) != invalid_maps.end()) {
                    text << "\t.4byte NULL\n";
                    continue;
                }
                begin_rom_world(text, worlds.at(name));
                text << "\t.4byte " << name << "\n"
                     << "\t.else\n\t.4byte NULL\n\t.endif\n";
            }
            text << "\n";
            valid_groups.push_back(group);
        }
    }

    text << "\t.align 2\n" << "gMapGroups::\n";
    for (auto &group : groups_data["group_order"].array_items()) {
        string group_str = json_to_string(group);
        if (find(valid_groups.begin(), valid_groups.end(), group_str) != valid_groups.end())
            text << "\t.4byte " << group_str << "\n";
        else
            text << "\t.4byte NULL\n";
    }
    text << "\n";

    return text.str();
}

string generate_connections_text(Json groups_data, vector<string> &invalid_maps, string include_path, const map<string, int> &worlds) {
    vector<Json> map_names;

    for (auto &group : groups_data["group_order"].array_items()) {
        for (auto map_name : groups_data[json_to_string(group)].array_items()) {
            string map_name_str = json_to_string(map_name);
            auto it = find(invalid_maps.begin(), invalid_maps.end(), map_name_str);
            if (it == invalid_maps.end())
                map_names.push_back(map_name);
        }
    }

    vector<Json> connections_include_order = groups_data["connections_include_order"].array_items();

    if (connections_include_order.size() > 0)
        sort(map_names.begin(), map_names.end(), [connections_include_order](const Json &a, const Json &b) {
            auto iter_a = find(connections_include_order.begin(), connections_include_order.end(), a);
            if (iter_a == connections_include_order.end())
                iter_a = connections_include_order.begin() + numeric_limits<int>::max();
            auto iter_b = find(connections_include_order.begin(), connections_include_order.end(), b);
            if (iter_b == connections_include_order.end())
                iter_b = connections_include_order.begin() + numeric_limits<int>::max();
            return iter_a < iter_b;
        });

    ostringstream text;

    text << get_generated_warning("data/maps/map_groups.json", true);

    for (Json map_name : map_names) {
        begin_rom_world(text, worlds.at(json_to_string(map_name)));
        text << "\t.include \"" << include_path << "/" <<  json_to_string(map_name) << "/connections.inc\"\n";
        text << "\t.endif\n";
    }

    return text.str();
}

string generate_headers_text(Json groups_data, vector<string> &invalid_maps, string include_path, const map<string, int> &worlds) {
    vector<string> map_names;

    for (auto &group : groups_data["group_order"].array_items()) {
        for (auto map_name : groups_data[json_to_string(group)].array_items()) {
            string map_name_str = json_to_string(map_name);
            auto it = find(invalid_maps.begin(), invalid_maps.end(), map_name_str);
            if (it == invalid_maps.end())
                map_names.push_back(json_to_string(map_name));
        }
    }

    ostringstream text;

    text << get_generated_warning("data/maps/map_groups.json", true);

    for (string map_name : map_names) {
        begin_rom_world(text, worlds.at(map_name));
        text << "\t.include \"" << include_path << "/" << map_name << "/header.inc\"\n";
        text << "\t.endif\n";
    }

    return text.str();
}

string generate_events_text(Json groups_data, vector<string> &invalid_maps, string include_path, const map<string, int> &worlds) {
    vector<string> map_names;

    for (auto &group : groups_data["group_order"].array_items()) {
        for (auto map_name : groups_data[json_to_string(group)].array_items()) {

            string map_name_str = json_to_string(map_name);
            auto it = find(invalid_maps.begin(), invalid_maps.end(), map_name_str);
            if (it == invalid_maps.end())
                map_names.push_back(json_to_string(map_name));
        }
    }

    ostringstream text;

    text << get_generated_warning(include_path + "/map_groups.json", true);

    for (string map_name : map_names) {
        begin_rom_world(text, worlds.at(map_name));
        text << "\t.include \"" << include_path << "/" << map_name << "/events.inc\"\n";
        text << "\t.endif\n";
    }

    return text.str();
}

Json parse_required_map_defines(void) {
    string json_err;

    string json_text = read_text_file("tools/mapjson/required_map_defines.json");

    Json json_data = Json::parse(json_text, json_err);
    if (json_data == Json())
        FATAL_ERROR("%s\n", json_err.c_str());
    return json_data;
}

string generate_map_constants_text(string groups_filepath, Json groups_data, vector<string> &valid_map_ids) {
    string file_dir = file_parent(groups_filepath) + sep;

    string guard_name = "CONSTANTS_MAP_GROUPS";
    ostringstream text;
    ostringstream mapCountText;

    text << get_include_guard_start(guard_name) << get_generated_warning("data/maps/map_groups.json", false);

    text << "//\n// DO NOT MODIFY THIS FILE! It is auto-generated from data/maps/map_groups.json\n//\n\n";

    text << "enum\n{\n";

    int group_num = 0;
    vector<int> map_count_vec; //DEBUG
    for (auto &group : groups_data["group_order"].array_items()) {
        string groupName = json_to_string(group);
        text << "    // " << groupName << "\n";
        vector<string> map_ids;
        size_t max_length = 0;

        int map_count = 0; //DEBUG

        for (auto &map_name : groups_data[groupName].array_items()) {
            string map_filepath = file_dir + json_to_string(map_name) + sep + "map.json";
            string err_str;
            Json map_data = Json::parse(read_text_file(map_filepath), err_str);
            if (map_data == Json())
                FATAL_ERROR("%s: %s\n", map_filepath.c_str(), err_str.c_str());
            string id = json_to_string(map_data, "id", true);
            map_ids.push_back(id);
            valid_map_ids.push_back(id);
            if (id.length() > max_length)
                max_length = id.length();
            map_count++; //DEBUG
        }

        int map_id_num = 0;
        for (string map_id : map_ids) {
            text << "    " << map_id << string(max_length - map_id.length(), ' ')
                 << " = (" << map_id_num++ << " | (" << group_num << " << 8)),\n";
        }

        text << "\n";

        group_num++;
        map_count_vec.push_back(map_count); //DEBUG
    }

    text << "};\n\n";

    text << "//Constants for unused maps\n";
    int map_id_num = 0;
    int old_map_group = -1;
    Json required_map_defines = parse_required_map_defines();
    map <int, string> filtered_map_defines;
    size_t max_length = 0;
    for (auto required_map_id : required_map_defines["required_maps"].array_items()) {
        string map_id = json_to_string(required_map_id[0]);
        auto it = find(valid_map_ids.begin(), valid_map_ids.end(), map_id);
        int current_map_group = required_map_id[1].int_value();
        if (old_map_group != current_map_group) {
            map_id_num = 0;
        } else {
            map_id_num++;
        }
        if (it == valid_map_ids.end()) {
            filtered_map_defines[(map_id_num + 256 * current_map_group)] = map_id;
            if (map_id.length() > max_length)
                max_length = map_id.length();
        }
        old_map_group = current_map_group;
    }

    for ( const auto &[map_value, map_id]: filtered_map_defines) {
        text << "#define " << map_id << string(max_length - map_id.length(), ' ')
             << "  " << map_value << "\n";
    }

    text << "\n#define MAP_GROUPS_COUNT " << group_num << "\n\n";
    text << get_include_guard_end(guard_name);

    char s = file_dir.back();
    mapCountText << "static const u8 MAP_GROUP_COUNT[] = {"; //DEBUG
    for(int i=0; i<group_num; i++){                          //DEBUG
        mapCountText << map_count_vec[i] << ", ";            //DEBUG
    }                                                        //DEBUG
    mapCountText << "0};\n";                                 //DEBUG
    write_text_file(file_dir + ".." + s + ".." + s + "src" + s + "data" + s + "map_group_count.h", mapCountText.str());

    return text.str();
}

void clean_heal_locations(vector<string> &valid_map_ids)
{
    std::stringstream new_json;
    std::ifstream infile("src/data/heal_locations.json");
    bool deleted_flag = false;

    std::regex map_regex("\"respawn_map\"\\s*:\\s*\"(MAP_\\w+)\"");
    std::regex npc_regex("LOCALID_\\w+");
    std::smatch map_match;
    string line;
    while (std::getline(infile, line))
    {
        if (std::regex_search(line, map_match, map_regex) && !deleted_flag) {
            auto it = find(valid_map_ids.begin(), valid_map_ids.end(), map_match[1]);
            if (it == valid_map_ids.end())
                deleted_flag = true;
        }
        if (deleted_flag && std::regex_search(line, npc_regex)) {
            deleted_flag = false;
            new_json << std::regex_replace(line, npc_regex, "0") << "\n";
        } else {
            new_json << line << "\n";
        }
    }

    write_text_file("src/data/heal_locations.json", new_json.str());
}

// Output paths are directories with trailing path separators
void process_groups(string groups_filepath, vector<string> &map_filepaths, string output_asm, string output_c) {
    output_asm = strip_trailing_separator(output_asm); // Remove separator if existing.
    output_c = strip_trailing_separator(output_c);

    string err;
    Json groups_data = Json::parse(read_text_file(groups_filepath), err);
    vector<string> invalid_maps;
    vector<string> valid_map_ids;

    if (groups_data == Json())
        FATAL_ERROR("%s\n", err.c_str());

    map<string, int> worlds;
    const string maps_dir = file_parent(groups_filepath) + sep;
    for (const auto &group : groups_data["group_order"].array_items()) {
        for (const auto &map_name : groups_data[json_to_string(group)].array_items()) {
            const string name = json_to_string(map_name);
            const string path = maps_dir + name + sep + "map.json";
            const Json data = Json::parse(read_text_file(path), err);
            if (data == Json())
                FATAL_ERROR("%s: %s\n", path.c_str(), err.c_str());
            worlds[name] = rom_world_mask(data, path);
        }
    }
    string groups_text = generate_groups_text(groups_data, invalid_maps, worlds);
    string connections_text = generate_connections_text(groups_data, invalid_maps, output_asm, worlds);
    string headers_text = generate_headers_text(groups_data, invalid_maps, output_asm, worlds);
    string events_text = generate_events_text(groups_data, invalid_maps, output_asm, worlds);
    string map_header_text = generate_map_constants_text(groups_filepath, groups_data, valid_map_ids);

    clean_heal_locations(valid_map_ids);
    write_text_file(output_asm + sep + "groups.inc", groups_text);
    write_text_file(output_asm + sep + "connections.inc", connections_text);
    write_text_file(output_asm + sep + "headers.inc", headers_text);
    write_text_file(output_asm + sep + "events.inc", events_text);
    write_text_file(output_c + sep + "map_groups.h", map_header_text);
}

string generate_layout_headers_text(Json layouts_data) {
    ostringstream text;

    text << get_generated_warning("data/layouts/layouts.json", true);

    for (auto &layout : layouts_data["layouts"].array_items()) {
        if (layout == Json::object()) continue;
        if (!std::filesystem::exists(json_to_string(layout, "border_filepath")))
            continue;
        string layout_version = json_to_string(layout, "layout_version", true);

        if (layout_version.empty()) {
            layout_version = "emerald";
        }
        string layoutName = json_to_string(layout, "name");
        begin_rom_world(text, rom_world_mask(layout, layoutName));
        string border_label = layoutName + "_Border";
        string blockdata_label = layoutName + "_Blockdata";
        text << border_label << "::\n"
             << "\t.incbin \"" << json_to_string(layout, "border_filepath") << "\"\n\n"
             << blockdata_label << "::\n"
             << "\t.incbin \"" << json_to_string(layout, "blockdata_filepath") << "\"\n\n"
             << "\t.align 2\n"
             << layoutName << "::\n"
             << "\t.4byte " << json_to_string(layout, "width") << "\n"
             << "\t.4byte " << json_to_string(layout, "height") << "\n"
             << "\t.4byte " << border_label << "\n"
             << "\t.4byte " << blockdata_label << "\n"
             << "\t.4byte " << json_to_string(layout, "primary_tileset") << "\n"
             << "\t.4byte " << json_to_string(layout, "secondary_tileset") << "\n";
        if (layout_version == "frlg")
            text << "\t.byte TRUE\n";
        else
            text << "\t.byte FALSE\n";

        if (layout_version == "frlg")
        {
            text << "\t.byte " << json_to_string(layout, "border_width") << "\n"
                 << "\t.byte " << json_to_string(layout, "border_height") << "\n"
                 << "\t.byte 0\n";
        }
        else
        {
            text << "\t.2byte 0\n"
                 << "\t.byte 0\n";
        }
        text << "\t.endif\n\n";
    }

    return text.str();
}

string generate_layouts_table_text(Json layouts_data) {
    ostringstream text;

    text << get_generated_warning("data/layouts/layouts.json", true);

    text << "\t.align 2\n"
         << json_to_string(layouts_data, "layouts_table_label") << "::\n";

    for (auto &layout : layouts_data["layouts"].array_items()) {
        if (!std::filesystem::exists(json_to_string(layout, "border_filepath")))
            continue;
        string layout_version = json_to_string(layout, "layout_version", true);
        if (layout_version.empty()) {
            layout_version = "emerald";
        }
        string layout_name = json_to_string(layout, "name", true);
        if (layout_name.empty()) layout_name = "NULL";
        begin_rom_world(text, rom_world_mask(layout, layout_name));
        text << "\t.4byte " << layout_name << "\n";
        text << "\t.else\n\t.4byte NULL\n\t.endif\n";
    }

    return text.str();
}

vector<string> parse_required_layout_defines()
{
    vector<string> v;
    string json_err;

    string json_text = read_text_file("tools/mapjson/required_map_defines.json");

    Json json_data = Json::parse(json_text, json_err);
    if (json_data == Json())
        FATAL_ERROR("%s\n", json_err.c_str());

    for (auto required_layout : json_data["required_layouts"].array_items()) {
        v.push_back(json_to_string(required_layout));
    }

    return v;
}
string generate_layouts_constants_text(Json layouts_data) {
    string guard_name = "CONSTANTS_LAYOUTS";
    ostringstream text;
    vector<string> defined_layouts;
    text << get_include_guard_start(guard_name) << get_generated_warning("data/layouts/layouts.json", false);

    int i = 1;
    for (auto &layout : layouts_data["layouts"].array_items()) {
        if (!std::filesystem::exists(json_to_string(layout, "border_filepath")))
            continue;
        if (layout != Json::object())
        {
            text << "#define " << json_to_string(layout, "id") << " " << i << "\n";
            defined_layouts.push_back(json_to_string(layout, "id"));
        }
        i++;
    }

    text << "\n//Constants for unused layouts\n";
    vector<string> required_layout_defines = parse_required_layout_defines();
    vector<string> filtered_layout_defines;
    size_t max_length = 0;
    for (auto &layout : required_layout_defines) {
        auto it = find(defined_layouts.begin(), defined_layouts.end(), layout);
        if (it == defined_layouts.end()) {
            filtered_layout_defines.push_back(layout);
            if (layout.length() > max_length)
                max_length = layout.length();
        }
    }

    for (auto &layout : filtered_layout_defines) {
        text << "#define " << layout << string(max_length - layout.length(), ' ')
             << "  0xFFFF\n";
    }
    text << "\n" << get_include_guard_end(guard_name);

    return text.str();
}

void process_layouts(string layouts_filepath, string output_asm, string output_c) {
    output_asm = strip_trailing_separator(output_asm).append(sep);
    output_c = strip_trailing_separator(output_c).append(sep);

    string err;
    Json layouts_data = Json::parse(read_text_file(layouts_filepath), err);

    if (layouts_data == Json())
        FATAL_ERROR("%s\n", err.c_str());

    string layout_headers_text = generate_layout_headers_text(layouts_data);
    string layouts_table_text = generate_layouts_table_text(layouts_data);
    string layouts_constants_text = generate_layouts_constants_text(layouts_data);

    write_text_file(output_asm + "layouts.inc", layout_headers_text);
    write_text_file(output_asm + "layouts_table.inc", layouts_table_text);
    write_text_file(output_c + "layouts.h", layouts_constants_text);
}

int main(int argc, char *argv[]) {
    // Large Crossroads map lists exceed Windows' process argument limit.
    // Response files contain the same ordered, whitespace-separated paths.
    vector<string> arguments;
    for (int i = 0; i < argc; ++i) {
        if (i > 0 && argv[i][0] == '@') {
            std::istringstream input(read_text_file(string(argv[i] + 1)));
            string argument;
            while (input >> argument)
                arguments.push_back(argument);
        } else {
            arguments.push_back(argv[i]);
        }
    }
    vector<char *> expanded;
    for (auto &argument : arguments)
        expanded.push_back(&argument[0]);
    argc = static_cast<int>(expanded.size());
    argv = expanded.data();
    if (argc < 3)
        FATAL_ERROR("USAGE: mapjson <mode> <game-version> [options]\n");

    char *version_arg = argv[2];
    version = string(version_arg);
    if (version != "emerald" && version != "ruby" && version != "firered")
        FATAL_ERROR("ERROR: <game-version> must be 'emerald', 'firered', or 'ruby'.\n");

    char *mode_arg = argv[1];
    string mode(mode_arg);
    if (mode == "map") {
        if (argc != 6)
            FATAL_ERROR("USAGE: mapjson map <game-version> <map_file> <layouts_file> <output_dir>\n");

        infer_separator(argv[3]);
        string filepath(argv[3]);
        string layouts_filepath(argv[4]);
        string output_dir(argv[5]);

        process_map(filepath, layouts_filepath, output_dir);
    }
    else if (mode == "groups") {
        if (argc < 6)
            FATAL_ERROR("USAGE: mapjson groups <game-version> <groups_file> <map_file> [additional_map_files] <output_asm_dir> <output_c_dir>\n");

        infer_separator(argv[3]);
        string filepath(argv[3]);

        vector<string> map_filepaths;
        const int firstMapFileArg = 4;
        const int lastMapFileArg = argc - 3;
        for (int i = firstMapFileArg; i <= lastMapFileArg; i++) {
            map_filepaths.push_back(argv[i]);
        }

        string output_asm(argv[argc - 2]);
        string output_c(argv[argc - 1]);

        process_groups(filepath, map_filepaths, output_asm, output_c);
    }
    else if (mode == "layouts") {
        if (argc != 6)
            FATAL_ERROR("USAGE: mapjson layouts <game-version> <layouts_file> <output_asm_dir> <output_c_dir>\n");

        infer_separator(argv[3]);
        string filepath(argv[3]);
        string output_asm(argv[4]);
        string output_c(argv[5]);

        process_layouts(filepath, output_asm, output_c);
    }
    else if (mode == "event_constants") {
        if (argc < 5)
            FATAL_ERROR("USAGE: mapjson event_constants <game-version> <map_file> [additional_map_files] <output_ids_file>");

        infer_separator(argv[3]);

        vector<string> filepaths;
        const int firstMapFileArg = 3;
        const int lastMapFileArg = argc - 2;
        for (int i = firstMapFileArg; i <= lastMapFileArg; i++) {
            filepaths.push_back(argv[i]);
        }
        string output_ids_file(argv[argc - 1]);

        process_event_constants(filepaths, output_ids_file);
    }
    else {
        FATAL_ERROR("ERROR: <mode> must be 'layouts', 'map', 'event_constants', or 'groups'.\n");
    }

    return 0;
}
